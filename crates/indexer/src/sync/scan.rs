//! Full scan: reconciles every project folder in the library with the database.
//! Files on disk are never modified.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lima_db::queries::{library_index as idx, sync as runs};
use lima_domain::models::asset::asset_kind;
use lima_domain::models::sync::SyncStats;
use serde_json::json;
use sqlx::{Pool, Sqlite};

use super::fs_walk::{self, FsFile};
use super::now;

#[derive(Debug)]
pub enum ScanError {
    LibraryMissing(PathBuf, String),
    /// Refused to drop every indexed project because the library looks empty
    /// (likely an unmounted volume rather than a real deletion).
    LibraryEmpty { indexed: usize },
    Db(sqlx::Error),
    Io(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::LibraryMissing(path, e) => write!(f, "Library folder {} is not readable: {e}", path.display()),
            ScanError::LibraryEmpty { indexed } => write!(
                f,
                "Library folder is empty but {indexed} project(s) are indexed; refusing to remove them all. Is the library mounted?"
            ),
            ScanError::Db(e) => write!(f, "Database error: {e}"),
            ScanError::Io(e) => write!(f, "Filesystem error: {e}"),
        }
    }
}

impl From<sqlx::Error> for ScanError {
    fn from(e: sqlx::Error) -> Self {
        ScanError::Db(e)
    }
}

#[derive(Debug, Default)]
struct ProjectChanges {
    added: u64,
    updated: u64,
    removed: u64,
}

pub(crate) struct Scanner<'a> {
    pool: &'a Pool<Sqlite>,
    root: &'a Path,
    run_id: &'a str,
    stats: SyncStats,
}

impl<'a> Scanner<'a> {
    pub fn new(pool: &'a Pool<Sqlite>, root: &'a Path, run_id: &'a str) -> Self {
        Self { pool, root, run_id, stats: SyncStats::default() }
    }

    /// Executes the scan. Stats are returned even when the run fails part way.
    pub async fn run(mut self) -> (SyncStats, Result<(), ScanError>) {
        let result = self.scan().await;
        if let Err(e) = &result {
            self.event("error", "run_failed", json!({ "error": e.to_string() })).await;
        }
        (self.stats, result)
    }

    async fn scan(&mut self) -> Result<(), ScanError> {
        let root = self.root.to_path_buf();
        let (dirs, skipped) = tokio::task::spawn_blocking(move || fs_walk::list_project_dirs(&root))
            .await
            .map_err(|e| ScanError::Io(e.to_string()))?
            .map_err(|e| ScanError::LibraryMissing(self.root.to_path_buf(), e.to_string()))?;

        for (name, reason) in skipped {
            self.skip_event(&name, &reason).await;
        }

        let indexed = idx::list_indexed_projects(self.pool).await?;

        if dirs.is_empty() && !indexed.is_empty() {
            return Err(ScanError::LibraryEmpty { indexed: indexed.len() });
        }

        self.stats.projects_total = dirs.len() as u64;
        self.event(
            "info",
            "run_started",
            json!({ "library": self.root.display().to_string(), "projects_on_disk": dirs.len(), "projects_indexed": indexed.len() }),
        )
        .await;
        self.save_stats().await;

        // Projects whose folder disappeared.
        let on_disk: HashSet<&str> = dirs.iter().map(String::as_str).collect();
        for project in indexed.iter().filter(|p| !on_disk.contains(p.folder_path.as_str())) {
            idx::remove_missing_project(self.pool, &project.id).await?;
            self.stats.projects_removed += 1;
            self.event(
                "warn",
                "project_removed",
                json!({ "project_id": project.id, "folder_path": project.folder_path }),
            )
            .await;
        }

        let by_folder: HashMap<&str, &idx::IndexedProject> =
            indexed.iter().map(|p| (p.folder_path.as_str(), p)).collect();

        for folder in &dirs {
            let existing = by_folder.get(folder.as_str()).copied();
            if let Err(e) = self.scan_project(folder, existing).await {
                // A broken project should not abort the whole run.
                self.stats.errors += 1;
                self.event("error", "project_failed", json!({ "folder_path": folder, "error": e.to_string() })).await;
            }
            self.stats.projects_scanned += 1;
            self.save_stats().await;
        }

        self.event("info", "run_finished", json!({ "stats": self.stats })).await;
        Ok(())
    }

    async fn scan_project(&mut self, folder: &str, existing: Option<&idx::IndexedProject>) -> Result<(), ScanError> {
        let dir = self.root.join(folder);
        let walk = tokio::task::spawn_blocking(move || fs_walk::walk_project(&dir))
            .await
            .map_err(|e| ScanError::Io(e.to_string()))?
            .map_err(|e| ScanError::Io(e.to_string()))?;

        for (path, reason) in &walk.skipped {
            self.skip_event(&format!("{folder}/{path}"), reason).await;
        }

        let now = now();
        let mut tx = self.pool.begin().await?;

        let (project_id, mut main_image_id, is_new) = match existing {
            Some(p) => (p.id.clone(), p.main_image_id.clone(), false),
            None => (idx::insert_discovered_project(&mut tx, folder, &now).await?, None, true),
        };

        let indexed_assets = if is_new { Vec::new() } else { idx::list_indexed_assets(&mut tx, &project_id).await? };
        let mut by_path: HashMap<String, idx::IndexedAsset> =
            indexed_assets.into_iter().map(|a| (a.file_path.clone(), a)).collect();

        let mut changes = ProjectChanges::default();
        let mut first_image: Option<String> = None;

        for file in &walk.files {
            let asset_id = match by_path.remove(&file.rel_path) {
                None => {
                    changes.added += 1;
                    insert_asset(&mut tx, &project_id, file, &now).await?
                }
                Some(asset) => {
                    if asset.size_bytes != file.size || asset.mtime != file.mtime {
                        idx::update_changed_asset(&mut tx, &asset.id, file.size, &file.mtime, &now).await?;
                        changes.updated += 1;
                    }
                    asset.id
                }
            };

            if first_image.is_none() && asset_kind(&file.rel_path) == "image" {
                first_image = Some(asset_id);
            }
        }

        // Whatever is left in the map no longer exists on disk.
        for asset in by_path.values() {
            idx::remove_missing_asset(&mut tx, &asset.id).await?;
            if main_image_id.as_deref() == Some(asset.id.as_str()) {
                main_image_id = None;
            }
            changes.removed += 1;
        }

        if main_image_id.is_none() {
            if let Some(image_id) = first_image {
                idx::set_main_image(&mut tx, &project_id, &image_id).await?;
            }
        }

        let changed = is_new || changes.added + changes.updated + changes.removed > 0;
        idx::mark_project_scanned(&mut tx, &project_id, changed, &now).await?;
        tx.commit().await?;

        self.stats.assets_added += changes.added;
        self.stats.assets_updated += changes.updated;
        self.stats.assets_removed += changes.removed;

        if is_new {
            self.stats.projects_added += 1;
            self.event(
                "info",
                "project_added",
                json!({ "project_id": project_id, "folder_path": folder, "assets": changes.added }),
            )
            .await;
        } else if changed {
            self.event(
                "info",
                "project_updated",
                json!({
                    "project_id": project_id,
                    "folder_path": folder,
                    "assets_added": changes.added,
                    "assets_updated": changes.updated,
                    "assets_removed": changes.removed,
                }),
            )
            .await;
        }

        Ok(())
    }

    async fn skip_event(&mut self, path: &str, reason: &str) {
        self.stats.errors += 1;
        self.event("warn", "entry_skipped", json!({ "path": path, "reason": reason })).await;
    }

    /// Event logging is best effort: a failure here should not fail the run.
    async fn event(&self, level: &str, kind: &str, data: serde_json::Value) {
        if let Err(e) = runs::insert_event(self.pool, self.run_id, level, kind, &data, &now()).await {
            tracing::warn!("Failed to record sync event {kind} for run {}: {e}", self.run_id);
        }
    }

    async fn save_stats(&self) {
        if let Err(e) = runs::update_run_stats(self.pool, self.run_id, &self.stats).await {
            tracing::warn!("Failed to update stats for run {}: {e}", self.run_id);
        }
    }
}

async fn insert_asset(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    project_id: &str,
    file: &FsFile,
    now: &str,
) -> Result<String, sqlx::Error> {
    let mime = mime_guess::from_path(&file.rel_path).first_or_octet_stream().to_string();
    idx::insert_discovered_asset(
        tx,
        project_id,
        &file.rel_path,
        asset_kind(&file.rel_path),
        file.size,
        &file.mtime,
        &mime,
        now,
    )
    .await
}
