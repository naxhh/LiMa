//! Metadata backup files on disk (`LIMA_BACKUP_DIR`) and the automatic backup task.

use std::io;
use std::path::Path;
use std::time::Duration;

use lima_domain::models::metadata::MetadataBackup;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use utoipa::ToSchema;

/// Backups kept by default; older automatic and manual ones are deleted.
pub const DEFAULT_KEEP: usize = 7;
/// Hours between automatic backups by default.
pub const DEFAULT_INTERVAL_HOURS: u64 = 24;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BackupFile {
    pub name: String,
    pub size_bytes: u64,
    /// When the file was written (RFC 3339).
    pub created_at: String,
}

pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

/// `metadata-2026-11-22T10-15-00Z.json`: sortable, and safe on every filesystem.
fn file_name(now: time::OffsetDateTime) -> String {
    format!(
        "metadata-{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z.json",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// Backup files, newest first.
pub fn list(dir: &Path) -> io::Result<Vec<BackupFile>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("metadata-") && name.ends_with(".json")) {
            continue;
        }
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let created_at = meta
            .modified()
            .ok()
            .and_then(|t| time::OffsetDateTime::from(t).format(&time::format_description::well_known::Rfc3339).ok())
            .unwrap_or_default();
        out.push(BackupFile { name, size_bytes: meta.len(), created_at });
    }
    // Names embed the UTC timestamp, so name order is time order.
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// The backup without its timestamp, to tell whether anything changed since the last one.
fn fingerprint(backup: &MetadataBackup) -> Option<String> {
    let mut b = backup.clone();
    b.exported_at = String::new();
    serde_json::to_string(&b).ok()
}

fn read(path: &Path) -> Option<MetadataBackup> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Exports the metadata and writes it as a new backup file, then prunes old ones.
/// With `only_if_changed`, nothing is written when the newest backup has the same content.
/// Returns the written file, or `None` when skipped.
pub async fn write_backup(
    pool: &Pool<Sqlite>,
    dir: &Path,
    keep: usize,
    only_if_changed: bool,
) -> Result<Option<BackupFile>, String> {
    let backup = lima_db::queries::metadata::export_metadata(pool, &now()).await.map_err(|e| e.to_string())?;
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        if only_if_changed {
            let latest = list(&dir).map_err(|e| e.to_string())?.into_iter().next();
            if let Some(latest) = latest {
                if read(&dir.join(&latest.name)).and_then(|b| fingerprint(&b)) == fingerprint(&backup) {
                    return Ok(None);
                }
            }
        }
        let name = file_name(time::OffsetDateTime::now_utc());
        let path = dir.join(&name);
        let tmp = dir.join(format!(".{name}.tmp"));
        let json = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, &json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
        prune(&dir, keep);
        let file = list(&dir).map_err(|e| e.to_string())?.into_iter().find(|f| f.name == name);
        Ok(file)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deletes all but the `keep` newest backups.
fn prune(dir: &Path, keep: usize) {
    let Ok(files) = list(dir) else { return };
    for old in files.into_iter().skip(keep.max(1)) {
        if let Err(e) = std::fs::remove_file(dir.join(&old.name)) {
            tracing::warn!("Could not delete old backup {}: {e}", old.name);
        }
    }
}

/// Settings for the automatic backup task.
#[derive(Debug, Clone, Copy)]
pub struct AutoBackup {
    pub interval: Duration,
    pub keep: usize,
}

impl AutoBackup {
    /// `LIMA_BACKUP_INTERVAL_HOURS` (0 disables) and `LIMA_BACKUP_KEEP`.
    pub fn from_env() -> Option<Self> {
        let num = |key: &str| std::env::var(key).ok().and_then(|v| v.trim().parse::<u64>().ok());
        let hours = num("LIMA_BACKUP_INTERVAL_HOURS").unwrap_or(DEFAULT_INTERVAL_HOURS);
        let keep = num("LIMA_BACKUP_KEEP").map(|k| k as usize).unwrap_or(DEFAULT_KEEP);
        (hours > 0).then(|| Self { interval: Duration::from_secs(hours * 3600), keep: keep.max(1) })
    }
}

/// Writes a backup a minute after startup and then every `interval`, skipping
/// when nothing changed since the newest backup.
pub fn spawn_auto_backup(pool: Pool<Sqlite>, dir: std::path::PathBuf, cfg: AutoBackup) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut tick = tokio::time::interval(cfg.interval);
        loop {
            tick.tick().await;
            match write_backup(&pool, &dir, cfg.keep, true).await {
                Ok(Some(f)) => tracing::info!("Metadata backup written: {}", f.name),
                Ok(None) => tracing::debug!("Metadata unchanged, no new backup"),
                Err(e) => tracing::error!("Metadata backup failed: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_sort_by_time() {
        let a = file_name(time::OffsetDateTime::from_unix_timestamp(1767323045).unwrap());
        let b = file_name(time::OffsetDateTime::from_unix_timestamp(1795341600).unwrap());
        assert_eq!(a, "metadata-2026-01-02T03-04-05Z.json");
        assert!(b > a);
    }
}
