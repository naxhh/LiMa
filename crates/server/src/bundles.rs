//! Bundles: files staged under `<state>/bundles/<id>/` with a `meta.json`, before they're
//! imported into a project (uploads, URL imports, extracted zips).

use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use lima_domain::config::Paths;
use lima_domain::models::asset::asset_kind;
use lima_domain::models::bundle::{BundleMeta, FileMeta};
use time::OffsetDateTime;
use tokio::{fs, io::AsyncWriteExt};

/// A bundle being filled by an upload.
pub struct NewBundle {
    pub id: String,
    dir: PathBuf,
    files: Vec<FileMeta>,
    pub failed: Vec<String>,
}

/// One file being written into a bundle.
pub struct FileWriter {
    name: String,
    path: PathBuf,
    file: fs::File,
    size: i64,
}

impl NewBundle {
    pub async fn create(paths: &Paths) -> std::io::Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = paths.bundle(&id).expect("generated bundle ids are UUIDs");
        fs::create_dir_all(&dir).await?;
        Ok(Self { id, dir, files: Vec::new(), failed: Vec::new() })
    }

    /// Starts a file. `None` (recorded as failed) when the name isn't a plain file name or
    /// the file can't be created.
    pub async fn start_file(&mut self, raw_name: &str) -> Option<FileWriter> {
        let Some(name) = sanitize_filename(raw_name) else {
            tracing::warn!("Invalid file name in an upload: {raw_name:?}");
            self.failed.push(raw_name.to_string());
            return None;
        };
        let path = self.dir.join(&name);
        match fs::File::create(&path).await {
            Ok(file) => Some(FileWriter { name, path, file, size: 0 }),
            Err(e) => {
                tracing::error!("Failed to create {}: {e}", path.display());
                self.failed.push(name);
                None
            }
        }
    }

    /// Records a finished file, or a failed one (its partial data is removed).
    pub async fn end_file(&mut self, writer: FileWriter, result: Result<(), String>, original_mtime_ms: Option<u64>) {
        let FileWriter { name, path, mut file, size } = writer;
        if let Err(e) = result {
            tracing::error!("Upload of {name} failed: {e}");
            drop(file);
            let _ = fs::remove_file(&path).await;
            self.failed.push(name);
            return;
        }
        if let Some(ms) = original_mtime_ms {
            keep_original_date(&mut file, ms, &name).await;
        }
        self.files.push(FileMeta {
            mtime: extract_mtime(&file).await.ok(),
            mime: guess_mime(&name),
            kind: asset_kind(&name).to_string(),
            checksum: compute_checksum(path).await.ok(),
            name,
            size,
        });
    }

    /// Writes `meta.json`. With no valid file the bundle is removed and `Ok(None)` returned.
    pub async fn finish(self) -> std::io::Result<Option<(String, Vec<String>, Vec<String>)>> {
        if self.files.is_empty() {
            let _ = fs::remove_dir_all(&self.dir).await;
            return Ok(None);
        }
        let names = self.files.iter().map(|f| f.name.clone()).collect();
        write_bundle_meta(&self.dir, self.files).await?;
        Ok(Some((self.id, names, self.failed)))
    }
}

impl FileWriter {
    pub async fn write(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        self.size += chunk.len() as i64;
        self.file.write_all(chunk).await
    }
}

/// A plain file name: no paths, no `..`.
pub fn sanitize_filename(name: &str) -> Option<String> {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return None;
    }
    Some(name.to_string())
}

pub async fn write_bundle_meta(bundle_dir: &Path, files: Vec<FileMeta>) -> Result<(), Error> {
    let meta = BundleMeta {
        uploaded_at: OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| "unknown".to_string()),
        files,
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;
    let mut file = fs::File::create(bundle_dir.join("meta.json")).await?;
    file.write_all(&json).await?;
    file.flush().await?;
    Ok(())
}

/// The metadata of a file already in a bundle folder (downloaded or extracted), stored as `name`.
pub async fn file_meta(path: PathBuf, name: &str) -> std::io::Result<FileMeta> {
    let file = fs::File::open(&path).await?;
    let size = file.metadata().await?.len() as i64;
    Ok(FileMeta {
        name: name.to_string(),
        size,
        mtime: extract_mtime(&file).await.ok(),
        mime: guess_mime(name),
        kind: asset_kind(name).to_string(),
        checksum: compute_checksum(path).await.ok(),
    })
}

pub fn guess_mime(path: &str) -> String {
    mime_guess::MimeGuess::from_path(path).first_or_octet_stream().to_string()
}

/// Sets an uploaded file's modification date to the one the browser reported.
async fn keep_original_date(file: &mut fs::File, ms: u64, name: &str) {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms);
    // Pending writes would bump the date again.
    let result = match file.flush().await {
        Ok(()) => match file.try_clone().await {
            Ok(f) => f.into_std().await.set_modified(time),
            Err(e) => Err(e),
        },
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        tracing::debug!("Could not keep the original date of {name}: {e}");
    }
}

async fn extract_mtime(file: &fs::File) -> Result<String, Error> {
    let metadata = file.metadata().await?;
    OffsetDateTime::from(metadata.modified()?)
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to format mtime: {e}")))
}

async fn compute_checksum(path: PathBuf) -> Result<String, Error> {
    tokio::task::spawn_blocking(move || lima_indexer::hash::hash_file(&path))
        .await
        .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    #[test]
    fn only_plain_file_names() {
        assert_eq!(sanitize_filename("benchy.stl").as_deref(), Some("benchy.stl"));
        assert_eq!(sanitize_filename("../x"), None);
        assert_eq!(sanitize_filename("a/b.stl"), None);
        assert_eq!(sanitize_filename(""), None);
    }
}
