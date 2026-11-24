//! tus uploads: each upload is a `<id>.part` file with the bytes received so far and an
//! `<id>.json` file with what it's for (bundle, file name, date) and its offset.
//!
//! When the last byte arrives the `.part` is moved into the bundle and listed in its
//! `meta.json`; the JSON is removed.

use std::io::Error;
use std::path::PathBuf;
use std::time::Duration;

use lima_domain::config::Paths;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::fs;

use crate::state::AppState;

/// Largest upload accepted, unless `LIMA_UPLOAD_MAX_BYTES` says otherwise.
pub const DEFAULT_MAX_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Uploads created this long ago are deleted by the hourly cleanup.
pub const MAX_AGE_HOURS: i64 = 24;

/// Reads `LIMA_UPLOAD_MAX_BYTES` (bytes). Zero or garbage means the default.
pub fn max_bytes_from_env() -> u64 {
    std::env::var("LIMA_UPLOAD_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// What an upload is for and how far it got.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadInfo {
    /// Bytes expected in total.
    pub length: i64,
    /// Bytes already written to the `.part` file.
    pub offset: i64,
    /// Bundle the finished file joins.
    pub bundle_id: String,
    /// Name the file gets in the bundle (already sanitized).
    pub filename: String,
    /// Original modification date, in ms since the epoch.
    pub mtime: Option<u64>,
    pub created_at: String,
}

/// The two files that make up an upload.
pub struct UploadFiles {
    pub info: PathBuf,
    pub part: PathBuf,
}

impl UploadFiles {
    /// The files of `id`, or `None` unless it looks like the ids we generate (so it can
    /// never escape the uploads folder).
    pub fn of(paths: &Paths, id: &str) -> Option<Self> {
        let valid = id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if !valid {
            return None;
        }
        let dir = paths.uploads();
        Some(Self { info: dir.join(format!("{id}.json")), part: dir.join(format!("{id}.part")) })
    }

    pub async fn read(&self) -> std::io::Result<Option<UploadInfo>> {
        match fs::read(&self.info).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| Error::other(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn save(&self, info: &UploadInfo) -> std::io::Result<()> {
        let json = serde_json::to_vec(info).map_err(|e| Error::other(e.to_string()))?;
        fs::write(&self.info, json).await
    }

    /// Removes the JSON and the partial file; missing files are fine.
    pub async fn remove(&self) -> std::io::Result<()> {
        remove_if_present(&self.info).await?;
        remove_if_present(&self.part).await
    }
}

async fn remove_if_present(path: &std::path::Path) -> std::io::Result<()> {
    match fs::remove_file(path).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Appends `.part` and `.json` for a new upload, created empty.
pub async fn create(
    paths: &Paths,
    length: i64,
    bundle_id: String,
    filename: String,
    mtime: Option<u64>,
) -> std::io::Result<(String, UploadFiles, UploadInfo)> {
    fs::create_dir_all(paths.uploads()).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let files = UploadFiles::of(paths, &id).expect("generated upload ids are valid");
    fs::File::create(&files.part).await?;
    let info = UploadInfo {
        length,
        offset: 0,
        bundle_id,
        filename,
        mtime,
        created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default(),
    };
    files.save(&info).await?;
    Ok((id, files, info))
}

/// An upload by id: its files and their info, or `None` when it's unknown.
pub async fn find(paths: &Paths, id: &str) -> std::io::Result<Option<(UploadFiles, UploadInfo)>> {
    let Some(files) = UploadFiles::of(paths, id) else {
        return Ok(None);
    };
    Ok(files.read().await?.map(|info| (files, info)))
}

/// Deletes uploads created more than [`MAX_AGE_HOURS`] ago. Returns how many went.
pub async fn cleanup(paths: &Paths) -> std::io::Result<usize> {
    let mut entries = match fs::read_dir(paths.uploads()).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let now = OffsetDateTime::now_utc();
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(files) = UploadFiles::of(paths, id) else {
            continue;
        };
        let Some(info) = files.read().await? else {
            continue;
        };
        let old = OffsetDateTime::parse(&info.created_at, &Rfc3339)
            .map(|created| (now - created).whole_hours() >= MAX_AGE_HOURS)
            .unwrap_or(false);
        if old {
            files.remove().await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Runs [`cleanup`] now, then every hour.
pub fn spawn_cleanup(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match cleanup(&state.paths).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Removed {n} abandoned upload(s)"),
                Err(e) => tracing::warn!("Cleaning up abandoned uploads failed: {e}"),
            }
        }
    });
}
