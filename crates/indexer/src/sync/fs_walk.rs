//! Blocking filesystem helpers. Call them through `spawn_blocking`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// A regular file found inside a project folder.
#[derive(Debug, Clone)]
pub struct FsFile {
    /// Path relative to the project folder, `/` separated.
    pub rel_path: String,
    pub size: i64,
    /// RFC3339, same format used by bundle uploads.
    pub mtime: String,
}

#[derive(Debug, Default)]
pub struct WalkResult {
    pub files: Vec<FsFile>,
    /// Entries that could not be indexed, with the reason.
    pub skipped: Vec<(String, String)>,
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Lists project folders: the non-hidden directories directly under the library root.
pub fn list_project_dirs(root: &Path) -> io::Result<(Vec<String>, Vec<(String, String)>)> {
    let mut dirs = Vec::new();
    let mut skipped = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(raw) => {
                skipped.push((raw.to_string_lossy().into_owned(), "Folder name is not valid UTF-8".into()));
                continue;
            }
        };
        if is_hidden(&name) {
            continue;
        }
        // Follows symlinks: a symlinked project folder is fine at top level.
        match fs::metadata(entry.path()) {
            Ok(meta) if meta.is_dir() => dirs.push(name),
            Ok(_) => {} // loose files at the library root are not projects
            Err(e) => skipped.push((name, e.to_string())),
        }
    }

    dirs.sort();
    Ok((dirs, skipped))
}

/// Recursively lists regular files of a project folder. Hidden entries and
/// symlinked directories (loop risk) are ignored.
pub fn walk_project(project_dir: &Path) -> io::Result<WalkResult> {
    let mut result = WalkResult::default();
    walk_dir(project_dir, &PathBuf::new(), &mut result)?;
    result.files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(result)
}

fn walk_dir(dir: &Path, rel: &Path, out: &mut WalkResult) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let rel_path = rel.join(entry.file_name());
        let display = rel_path.to_string_lossy().replace('\\', "/");

        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            out.skipped.push((display, "File name is not valid UTF-8".into()));
            continue;
        };
        if is_hidden(&name) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                out.skipped.push((display, e.to_string()));
                continue;
            }
        };

        if file_type.is_dir() {
            if let Err(e) = walk_dir(&entry.path(), &rel_path, out) {
                out.skipped.push((display, e.to_string()));
            }
            continue;
        }

        // Regular files and symlinks pointing to files.
        let meta = match fs::metadata(entry.path()) {
            Ok(meta) => meta,
            Err(e) => {
                out.skipped.push((display, e.to_string()));
                continue;
            }
        };
        if !meta.is_file() {
            continue;
        }

        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| OffsetDateTime::from(t).format(&Rfc3339).ok())
            .unwrap_or_default();

        out.files.push(FsFile {
            rel_path: display,
            size: meta.len() as i64,
            mtime,
        });
    }
    Ok(())
}
