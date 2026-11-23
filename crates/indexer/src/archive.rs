//! Zip archives: list what's inside, read one entry, and extract safely (uploads only).
//!
//! Files already in the library are never extracted; they're only read.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

/// Entries listed for one archive; bigger archives are cut off (`truncated`).
pub const MAX_LISTED: usize = 5_000;

/// Guards for extraction, against zip bombs.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    /// Total bytes written, counted as they're decompressed (headers can lie).
    pub max_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_files: 10_000, max_bytes: 8 * 1024 * 1024 * 1024 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Path inside the archive, `/`-separated.
    pub path: String,
    pub size: u64,
    pub compressed_size: u64,
    pub encrypted: bool,
    /// Whether this build can decompress it.
    pub supported: bool,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// More files than `MAX_LISTED`.
    pub truncated: bool,
}

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    /// Not a zip, or a broken one.
    Invalid(String),
    NotFound,
    Encrypted,
    Unsupported,
    TooLarge,
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveError::Io(e) => write!(f, "{e}"),
            ArchiveError::Invalid(e) => write!(f, "not a readable zip: {e}"),
            ArchiveError::NotFound => write!(f, "no such file in the archive"),
            ArchiveError::Encrypted => write!(f, "the file is password protected"),
            ArchiveError::Unsupported => write!(f, "unsupported compression"),
            ArchiveError::TooLarge => write!(f, "the archive is too large to extract"),
        }
    }
}

impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self {
        ArchiveError::Io(e)
    }
}

impl From<zip::result::ZipError> for ArchiveError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(e) => ArchiveError::Io(e),
            zip::result::ZipError::FileNotFound => ArchiveError::NotFound,
            zip::result::ZipError::UnsupportedArchive(m) if m.contains("assword") || m.contains("ncrypt") => ArchiveError::Encrypted,
            zip::result::ZipError::UnsupportedArchive(_) => ArchiveError::Unsupported,
            other => ArchiveError::Invalid(other.to_string()),
        }
    }
}

pub fn is_zip(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".zip")
}

/// Clutter archivers add that nobody wants as files.
fn is_junk(path: &str) -> bool {
    path.starts_with("__MACOSX/") || path.split('/').any(|p| p == ".DS_Store" || p == "Thumbs.db" || p == "desktop.ini")
}

fn open(path: &Path) -> Result<ZipArchive<File>, ArchiveError> {
    Ok(ZipArchive::new(File::open(path)?)?)
}

/// An entry's date (zip stores local time without a zone; read as UTC).
fn zip_time(t: zip::DateTime) -> Option<time::OffsetDateTime> {
    let month = time::Month::try_from(t.month()).ok()?;
    let date = time::Date::from_calendar_date(t.year() as i32, month, t.day()).ok()?;
    let clock = time::Time::from_hms(t.hour(), t.minute(), t.second()).ok()?;
    Some(time::PrimitiveDateTime::new(date, clock).assume_utc())
}

/// Compression methods this build decompresses.
fn decodable(method: zip::CompressionMethod) -> bool {
    use zip::CompressionMethod::*;
    matches!(method, Stored | Deflated | Deflate64 | Bzip2 | Zstd | Lzma | Xz)
}

const SYMLINK: u32 = 0o120000;

fn is_symlink(file: &zip::read::ZipFile<'_, File>) -> bool {
    file.unix_mode().is_some_and(|m| m & 0o170000 == SYMLINK)
}

/// Files in the archive (folders, links and clutter left out), in archive order.
pub fn list(path: &Path) -> Result<Listing, ArchiveError> {
    let mut archive = open(path)?;
    let mut out = Listing::default();
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i)?;
        if file.is_dir() || is_symlink(&file) {
            continue;
        }
        let Some(name) = file.enclosed_name() else { continue };
        let path = name.to_string_lossy().replace('\\', "/");
        if is_junk(&path) {
            continue;
        }
        if out.entries.len() == MAX_LISTED {
            out.truncated = true;
            break;
        }
        out.entries.push(Entry {
            supported: decodable(file.compression()) && !file.encrypted(),
            encrypted: file.encrypted(),
            size: file.size(),
            compressed_size: file.compressed_size(),
            path,
        });
    }
    Ok(out)
}

/// Copies one file out of the archive into `out`, stopping after `max_bytes`.
pub fn read_entry(archive_path: &Path, entry: &str, out: &mut impl Write, max_bytes: u64) -> Result<u64, ArchiveError> {
    let mut archive = open(archive_path)?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index_raw(i)
                .ok()
                .and_then(|f| f.enclosed_name().map(|n| n.to_string_lossy().replace('\\', "/")))
                .is_some_and(|n| n == entry)
        })
        .ok_or(ArchiveError::NotFound)?;
    let file = archive.by_index(index)?;
    if file.encrypted() {
        return Err(ArchiveError::Encrypted);
    }
    let written = io::copy(&mut file.take(max_bytes.saturating_add(1)), out)?;
    if written > max_bytes {
        return Err(ArchiveError::TooLarge);
    }
    Ok(written)
}

#[derive(Debug, Default)]
pub struct Extracted {
    /// Paths written, relative to the destination, `/`-separated.
    pub files: Vec<String>,
    /// Entries left out (password protected, unsupported compression), by archive path.
    pub skipped: Vec<String>,
}

/// First name that's free under `dest` and not already taken in `taken`: `a.stl`, `a (2).stl`, ...
fn free_name(dest: &Path, wanted: &str, taken: &std::collections::HashSet<String>) -> String {
    let free = |n: &str| !taken.contains(&n.to_ascii_lowercase()) && !dest.join(n).exists();
    if free(wanted) {
        return wanted.to_string();
    }
    let (dir, file) = wanted.rsplit_once('/').map(|(d, f)| (format!("{d}/"), f)).unwrap_or((String::new(), wanted));
    let (stem, ext) = match file.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (file, String::new()),
    };
    (2..).map(|n| format!("{dir}{stem} ({n}){ext}")).find(|n| free(n)).expect("some name is free")
}

/// Extracts every file into `dest`, keeping the archive's folders. Never overwrites: a taken
/// name gets a ` (2)` suffix. `taken` holds names (lowercase) already claimed by other files
/// going to the same place. Rejects `../` paths and links, and stops at `limits`.
/// On error, files written so far are removed.
pub fn extract(
    archive_path: &Path,
    dest: &Path,
    limits: Limits,
    taken: &mut std::collections::HashSet<String>,
) -> Result<Extracted, ArchiveError> {
    let mut archive = open(archive_path)?;
    let mut out = Extracted::default();
    let mut written_paths: Vec<PathBuf> = Vec::new();
    let mut budget = limits.max_bytes;

    let result = (|| -> Result<(), ArchiveError> {
        for i in 0..archive.len() {
            // Look at the header first: encrypted entries can't be opened for reading.
            let name = {
                let raw = archive.by_index_raw(i)?;
                if raw.is_dir() || is_symlink(&raw) {
                    continue;
                }
                let Some(name) = raw.enclosed_name() else { continue };
                let name = name.to_string_lossy().replace('\\', "/");
                if is_junk(&name) {
                    continue;
                }
                if raw.encrypted() || !decodable(raw.compression()) {
                    out.skipped.push(name);
                    continue;
                }
                name
            };
            if out.files.len() == limits.max_files {
                return Err(ArchiveError::TooLarge);
            }
            let rel = free_name(dest, &name, taken);
            let target = dest.join(&rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = archive.by_index(i)?;
            let modified = file.last_modified().and_then(zip_time);
            let mut f = File::create_new(&target)?;
            written_paths.push(target.clone());
            let n = io::copy(&mut file.take(budget.saturating_add(1)), &mut f)?;
            if n > budget {
                return Err(ArchiveError::TooLarge);
            }
            budget -= n;
            if let Some(t) = modified {
                let _ = f.set_modified(t.into());
            }
            taken.insert(rel.to_ascii_lowercase());
            out.files.push(rel);
        }
        Ok(())
    })();

    if let Err(e) = result {
        for p in written_paths {
            let _ = std::fs::remove_file(p);
        }
        return Err(e);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use zip::write::SimpleFileOptions;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lima-archive-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data) in files {
            if name.ends_with('/') {
                w.add_directory(*name, SimpleFileOptions::default()).unwrap();
            } else {
                w.start_file(*name, SimpleFileOptions::default()).unwrap();
                w.write_all(data).unwrap();
            }
        }
        w.finish().unwrap();
    }

    #[test]
    fn lists_extracts_and_reads() {
        let dir = temp();
        let zip = dir.join("pack.zip");
        make_zip(&zip, &[("parts/", b""), ("parts/a.stl", b"solid a"), ("b.stl", b"solid b"), ("__MACOSX/._b.stl", b"x"), ("../evil.txt", b"x")]);

        let listing = list(&zip).unwrap();
        let paths: Vec<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["parts/a.stl", "b.stl"], "folders, clutter and ../ paths left out");

        let mut buf = Vec::new();
        assert_eq!(read_entry(&zip, "parts/a.stl", &mut buf, 1000).unwrap(), 7);
        assert_eq!(buf, b"solid a");
        assert!(matches!(read_entry(&zip, "nope", &mut Vec::new(), 1000), Err(ArchiveError::NotFound)));
        assert!(matches!(read_entry(&zip, "b.stl", &mut Vec::new(), 3), Err(ArchiveError::TooLarge)));

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("b.stl"), "already here").unwrap();
        let mut taken = HashSet::new();
        let got = extract(&zip, &dest, Limits::default(), &mut taken).unwrap();
        assert_eq!(got.files, vec!["parts/a.stl", "b (2).stl"]);
        assert_eq!(std::fs::read_to_string(dest.join("b.stl")).unwrap(), "already here", "never overwrites");
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stops_at_limits_and_cleans_up() {
        let dir = temp();
        let zip = dir.join("bomb.zip");
        let big = vec![0u8; 10_000];
        make_zip(&zip, &[("a.bin", &big), ("b.bin", &big)]);
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let err = extract(&zip, &dest, Limits { max_files: 10, max_bytes: 15_000 }, &mut HashSet::new()).unwrap_err();
        assert!(matches!(err, ArchiveError::TooLarge));
        assert!(!dest.join("a.bin").exists(), "partial output removed");
        let err = extract(&zip, &dest, Limits { max_files: 1, max_bytes: u64::MAX }, &mut HashSet::new()).unwrap_err();
        assert!(matches!(err, ArchiveError::TooLarge));
        let _ = std::fs::remove_dir_all(dir);
    }
}
