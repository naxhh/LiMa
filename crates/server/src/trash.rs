//! Trash: deleting a project moves its folder to `<library>/.lima-trash/<id>/` (a cheap rename on
//! the same filesystem; sync and the watcher skip hidden folders) and keeps its metadata, so it
//! can be restored completely. Items are purged after `LIMA_TRASH_DAYS` (default 30, 0 = no trash).

use std::path::{Path, PathBuf};
use std::time::Duration;

use lima_db::queries::trash::{self, TrashRow};
use lima_domain::models::metadata::MetadataBackup;
use lima_indexer::sync::{StartSyncError, SyncMode, SyncReason};

use crate::state::AppState;

pub const DEFAULT_DAYS: u64 = 30;
/// Hidden, so sync and the watcher ignore it.
pub const TRASH_DIR: &str = ".lima-trash";

/// `LIMA_TRASH_DAYS`: how long trashed projects are kept. 0 disables the trash (deletes are final).
pub fn days_from_env() -> u64 {
    std::env::var("LIMA_TRASH_DAYS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_DAYS)
}

#[derive(Debug)]
pub enum TrashError {
    NotFound,
    Io(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for TrashError {
    fn from(e: sqlx::Error) -> Self {
        TrashError::Db(e)
    }
}

fn io(context: &str, e: std::io::Error) -> TrashError {
    TrashError::Io(format!("{context}: {e}"))
}

fn trash_dir(library: &Path, id: &str) -> PathBuf {
    library.join(TRASH_DIR).join(id)
}

/// Where a trashed item's folder lives.
fn trashed_folder(library: &Path, row: &TrashRow) -> PathBuf {
    trash_dir(library, &row.id).join(&row.folder_path)
}

/// Moves a project to the trash. Returns the trash item id.
pub async fn trash_project(state: &AppState, project_id: &str) -> Result<String, TrashError> {
    let pool = state.db.pool();
    let now = crate::backups::now();
    let candidate = trash::candidate(pool, project_id).await?.ok_or(TrashError::NotFound)?;
    let snapshot = lima_db::queries::metadata::export_project(pool, project_id, &now).await?.ok_or(TrashError::NotFound)?;
    let id = uuid::Uuid::new_v4().to_string();
    let library = &state.paths.library;

    let from = state.paths.project_dir(&candidate.folder_path);
    let to = trash_dir(library, &id).join(&candidate.folder_path);
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| io("Creating the trash folder", e))?;
    }
    tokio::fs::rename(&from, &to).await.map_err(|e| io("Moving the folder to the trash", e))?;

    let row = TrashRow {
        id: id.clone(),
        folder_path: candidate.folder_path,
        name: candidate.name,
        file_count: candidate.file_count,
        size_bytes: candidate.size_bytes,
        snapshot: serde_json::to_string(&snapshot).expect("serialize snapshot"),
        trashed_at: now,
        file_path: None,
    };
    if let Err(e) = trash::trash_project(pool, project_id, &row).await {
        // Put the folder back so nothing is lost.
        let _ = tokio::fs::rename(&to, &from).await;
        let _ = tokio::fs::remove_dir_all(trash_dir(library, &id)).await;
        return Err(e.into());
    }
    remove_thumbnails(state, project_id).await;
    Ok(id)
}

/// What a trashed file remembers besides its path.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct FileSnapshot {
    #[serde(default)]
    cover: bool,
}

/// Moves one file of a project to the trash. Returns the trash item id.
pub async fn trash_file(state: &AppState, project_id: &str, asset_id: &str) -> Result<String, TrashError> {
    let pool = state.db.pool();
    let c = trash::file_candidate(pool, project_id, asset_id).await?.ok_or(TrashError::NotFound)?;
    let id = uuid::Uuid::new_v4().to_string();
    let library = &state.paths.library;
    let from = state.paths.project_dir(&c.folder_path).join(&c.file_path);
    let to = trash_dir(library, &id).join(&c.folder_path).join(&c.file_path);
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| io("Creating the trash folder", e))?;
    }
    match tokio::fs::rename(&from, &to).await {
        Ok(()) => {}
        // Already gone from disk: only the index entry is left to remove.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io("Moving the file to the trash", e)),
    }
    let row = TrashRow {
        id: id.clone(),
        folder_path: c.folder_path,
        name: c.project_name,
        file_count: 1,
        size_bytes: c.size_bytes,
        snapshot: serde_json::to_string(&FileSnapshot { cover: c.is_cover }).expect("serialize snapshot"),
        trashed_at: crate::backups::now(),
        file_path: Some(c.file_path),
    };
    if let Err(e) = trash::trash_file(pool, project_id, asset_id, &row).await {
        let _ = tokio::fs::rename(&to, &from).await;
        let _ = tokio::fs::remove_dir_all(trash_dir(library, &id)).await;
        return Err(e.into());
    }
    let thumbs = state.paths.thumbnails();
    let (project, asset) = (project_id.to_string(), asset_id.to_string());
    let _ = tokio::task::spawn_blocking(move || lima_indexer::thumbnails::remove_asset(&thumbs, &project, &asset)).await;
    Ok(id)
}

/// First free file name in `dir`: `a.stl`, `a-restored.stl`, `a-restored-2.stl`, ...
fn free_file(dir: &Path, wanted: &str) -> String {
    if !dir.join(wanted).exists() {
        return wanted.to_string();
    }
    let (stem, ext) = match wanted.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !s.ends_with('/') => (s.to_string(), format!(".{e}")),
        _ => (wanted.to_string(), String::new()),
    };
    (1..)
        .map(|n| if n == 1 { format!("{stem}-restored{ext}") } else { format!("{stem}-restored-{n}{ext}") })
        .find(|name| !dir.join(name).exists())
        .expect("some name is free")
}

/// Puts a trashed file back into its project folder (recreated if needed) and indexes it.
async fn restore_file(state: &AppState, row: &TrashRow, file_path: &str) -> Result<String, TrashError> {
    let pool = state.db.pool();
    let library = &state.paths.library;
    let dir = state.paths.project_dir(&row.folder_path);
    let target = free_file(&dir, file_path);
    let dest = dir.join(&target);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| io("Recreating the folder", e))?;
    }
    tokio::fs::rename(trash_dir(library, &row.id).join(&row.folder_path).join(file_path), &dest)
        .await
        .map_err(|e| io("Moving the file back", e))?;
    let _ = tokio::fs::remove_dir_all(trash_dir(library, &row.id)).await;
    trash::remove(pool, &row.id).await?;

    sync_folder(state, &row.folder_path).await?;
    let project_id = sqlx::query_scalar::<_, String>("SELECT id FROM projects WHERE folder_path = ?1")
        .bind(&row.folder_path)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| TrashError::Io(format!("{} was restored but couldn't be indexed", row.folder_path)))?;
    let snapshot: FileSnapshot = serde_json::from_str(&row.snapshot).unwrap_or_default();
    if snapshot.cover {
        sqlx::query("UPDATE projects SET main_image_id = (SELECT id FROM assets WHERE project_id = ?1 AND file_path = ?2) WHERE id = ?1 AND main_image_id IS NULL")
            .bind(&project_id)
            .bind(&target)
            .execute(pool)
            .await?;
    }
    Ok(project_id)
}

/// First free folder name: the original, else `<name>-restored`, `<name>-restored-2`, ...
fn free_folder(library: &Path, wanted: &str) -> String {
    if !library.join(wanted).exists() {
        return wanted.to_string();
    }
    (1..)
        .map(|n| if n == 1 { format!("{wanted}-restored") } else { format!("{wanted}-restored-{n}") })
        .find(|name| !library.join(name).exists())
        .expect("some name is free")
}

/// Indexes one folder now, waiting for any running sync first.
async fn sync_folder(state: &AppState, folder: &str) -> Result<(), TrashError> {
    loop {
        match state.sync.start_scoped(SyncMode::Incremental, SyncReason::Manual, Some(vec![folder.to_string()])).await {
            Ok(_) => break,
            Err(StartSyncError::AlreadyRunning { .. }) => state.sync.wait_idle().await,
            Err(StartSyncError::Db(e)) => return Err(e.into()),
        }
    }
    state.sync.wait_idle().await;
    Ok(())
}

/// Moves the folder back, indexes it and reapplies its metadata. Returns the project id.
pub async fn restore(state: &AppState, id: &str) -> Result<String, TrashError> {
    let pool = state.db.pool();
    let row = trash::get(pool, id).await?.ok_or(TrashError::NotFound)?;
    if let Some(file_path) = row.file_path.clone() {
        return restore_file(state, &row, &file_path).await;
    }
    let library = &state.paths.library;
    let folder = free_folder(library, &row.folder_path);
    tokio::fs::rename(trashed_folder(library, &row), library.join(&folder))
        .await
        .map_err(|e| io("Moving the folder back", e))?;
    let _ = tokio::fs::remove_dir_all(trash_dir(library, id)).await;
    trash::remove(pool, id).await?;

    sync_folder(state, &folder).await?;
    let mut snapshot: MetadataBackup = serde_json::from_str(&row.snapshot).map_err(|e| TrashError::Io(e.to_string()))?;
    for p in &mut snapshot.projects {
        p.folder_path = folder.clone();
    }
    if let Err(e) = lima_db::queries::metadata::restore_metadata(pool, &snapshot, false, &crate::backups::now()).await {
        tracing::warn!("Restored {folder} but couldn't reapply its metadata: {e:?}");
    }
    sqlx::query_scalar::<_, String>("SELECT id FROM projects WHERE folder_path = ?1")
        .bind(&folder)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| TrashError::Io(format!("{folder} was restored but couldn't be indexed")))
}

/// Deletes a trashed item for good.
pub async fn purge(state: &AppState, row: &TrashRow) -> Result<(), TrashError> {
    let dir = trash_dir(&state.paths.library, &row.id);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io("Deleting the trashed folder", e)),
    }
    trash::remove(state.db.pool(), &row.id).await?;
    Ok(())
}

/// Purges items trashed more than `days` ago. Returns how many.
pub async fn purge_expired(state: &AppState, days: u64) -> Result<usize, TrashError> {
    let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(days as i64))
        .format(&time::format_description::well_known::Rfc3339)
        .expect("format time");
    let expired = trash::older_than(state.db.pool(), &cutoff).await?;
    for row in &expired {
        purge(state, row).await?;
    }
    Ok(expired.len())
}

/// Purges expired items shortly after startup and then every hour.
pub fn spawn_auto_purge(state: AppState, days: u64) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match purge_expired(&state, days).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Emptied {n} expired item(s) from the trash"),
                Err(e) => tracing::error!("Emptying the trash failed: {e:?}"),
            }
        }
    });
}

async fn remove_thumbnails(state: &AppState, project_id: &str) {
    let thumbs = state.paths.thumbnails();
    let project = project_id.to_string();
    if let Ok(Err(e)) = tokio::task::spawn_blocking(move || lima_indexer::thumbnails::remove_project(&thumbs, &project)).await {
        tracing::warn!("Failed to remove thumbnails of project {project_id}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_folders_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("lima-trash-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("benchy")).unwrap();
        assert_eq!(free_folder(&dir, "dragon"), "dragon");
        assert_eq!(free_folder(&dir, "benchy"), "benchy-restored");
        std::fs::create_dir_all(dir.join("benchy-restored")).unwrap();
        assert_eq!(free_folder(&dir, "benchy"), "benchy-restored-2");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restored_files_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("lima-trash-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("parts")).unwrap();
        assert_eq!(free_file(&dir, "a.stl"), "a.stl");
        std::fs::write(dir.join("parts/a.stl"), "x").unwrap();
        assert_eq!(free_file(&dir, "parts/a.stl"), "parts/a-restored.stl");
        std::fs::write(dir.join("parts/a-restored.stl"), "x").unwrap();
        assert_eq!(free_file(&dir, "parts/a.stl"), "parts/a-restored-2.stl");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
