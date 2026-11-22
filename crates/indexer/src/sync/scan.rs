//! Full scan: reconciles every project folder in the library with the database.
//! Files on disk are never modified.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lima_db::queries::{library_index as idx, sync as runs};
use lima_domain::models::asset::asset_kind;
use lima_domain::models::sync::SyncStats;
use serde_json::json;
use sqlx::{Pool, Sqlite};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::fs_walk::{self, FsFile};
use crate::thumbnails;
use super::{SyncMode, now};

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

/// What to do with a file found on disk.
enum Action {
    Insert,
    /// Size or mtime differ from the index.
    Update { asset_id: String },
    /// Stat data unchanged.
    Keep { asset_id: String, stored_hash: Option<String> },
}

struct Planned<'f> {
    file: &'f FsFile,
    action: Action,
    needs_hash: bool,
}

pub(crate) struct Scanner<'a> {
    pool: &'a Pool<Sqlite>,
    root: &'a Path,
    thumbs_root: &'a Path,
    run_id: &'a str,
    mode: SyncMode,
    /// Only these project folders (watcher runs). `None` = whole library.
    scope: Option<HashSet<String>>,
    stats: SyncStats,
    /// Thumbnails render in the background while the scan moves on.
    thumb_tasks: JoinSet<ThumbOutcome>,
    thumb_permits: Arc<Semaphore>,
}

/// `None` when the thumbnail already existed and nothing was done.
type ThumbOutcome = (PathBuf, Option<Result<(), String>>);

/// An image asset that may need a thumbnail.
struct ThumbJob {
    asset_id: String,
    source: PathBuf,
    /// Content changed (or is new): regenerate even if a thumbnail exists.
    force: bool,
}

impl<'a> Scanner<'a> {
    pub fn new(
        pool: &'a Pool<Sqlite>,
        root: &'a Path,
        thumbs_root: &'a Path,
        run_id: &'a str,
        mode: SyncMode,
        scope: Option<HashSet<String>>,
    ) -> Self {
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        Self {
            pool,
            root,
            thumbs_root,
            run_id,
            mode,
            scope,
            stats: SyncStats::default(),
            thumb_tasks: JoinSet::new(),
            thumb_permits: Arc::new(Semaphore::new(workers)),
        }
    }

    /// Executes the scan. Stats are returned even when the run fails part way.
    pub async fn run(mut self) -> (SyncStats, Result<(), ScanError>) {
        let result = self.scan().await;
        // Let in-flight thumbnails finish even if the scan failed.
        self.drain_thumbs(true).await;
        if result.is_ok() {
            self.event("info", "run_finished", json!({ "stats": self.stats })).await;
        }
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

        let mut indexed = idx::list_indexed_projects(self.pool).await?;

        if dirs.is_empty() && !indexed.is_empty() {
            return Err(ScanError::LibraryEmpty { indexed: indexed.len() });
        }

        // Folders removed from LiMa with their files kept stay out of the index.
        // (Filtered after the empty-library check, which is about the disk.)
        let ignored = lima_db::queries::ignored_folders::ignored_set(self.pool).await?;
        let dirs: Vec<String> = dirs.into_iter().filter(|d| !ignored.contains(d)).collect();

        // A scoped run only looks at (and may only remove) the requested folders.
        let dirs: Vec<String> = match &self.scope {
            Some(scope) => {
                indexed.retain(|p| scope.contains(&p.folder_path));
                dirs.into_iter().filter(|d| scope.contains(d)).collect()
            }
            None => dirs,
        };
        let scope_list: Option<Vec<&String>> = self.scope.as_ref().map(|s| {
            let mut v: Vec<&String> = s.iter().collect();
            v.sort();
            v
        });

        self.stats.projects_total = dirs.len() as u64;
        self.event(
            "info",
            "run_started",
            json!({
                "mode": self.mode.as_str(),
                "library": self.root.display().to_string(),
                "folders": scope_list,
                "projects_on_disk": dirs.len(),
                "projects_indexed": indexed.len(),
            }),
        )
        .await;
        self.save_stats().await;

        // Projects whose folder disappeared.
        let on_disk: HashSet<&str> = dirs.iter().map(String::as_str).collect();
        for project in indexed.iter().filter(|p| !on_disk.contains(p.folder_path.as_str())) {
            idx::remove_missing_project(self.pool, &project.id).await?;
            self.remove_thumbs(&project.id, None).await;
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
            self.drain_thumbs(false).await;
            self.save_stats().await;
        }

        Ok(())
    }

    async fn scan_project(&mut self, folder: &str, existing: Option<&idx::IndexedProject>) -> Result<(), ScanError> {
        let project_dir = self.root.join(folder);
        let walk_dir = project_dir.clone();
        let walk = tokio::task::spawn_blocking(move || fs_walk::walk_project(&walk_dir))
            .await
            .map_err(|e| ScanError::Io(e.to_string()))?
            .map_err(|e| ScanError::Io(e.to_string()))?;

        for (path, reason) in &walk.skipped {
            self.skip_event(&format!("{folder}/{path}"), reason).await;
        }

        // 1. Diff disk against the index (read only, no transaction yet).
        let indexed_assets = match existing {
            Some(p) => idx::list_indexed_assets(self.pool, &p.id).await?,
            None => Vec::new(),
        };
        let mut by_path: HashMap<String, idx::IndexedAsset> =
            indexed_assets.into_iter().map(|a| (a.file_path.clone(), a)).collect();

        let verify_all = self.mode == SyncMode::Full;
        let plan: Vec<Planned> = walk
            .files
            .iter()
            .map(|file| {
                let action = match by_path.remove(&file.rel_path) {
                    None => Action::Insert,
                    Some(a) if a.size_bytes != file.size || a.mtime != file.mtime => Action::Update { asset_id: a.id },
                    Some(a) => Action::Keep { asset_id: a.id, stored_hash: a.file_hash },
                };
                let needs_hash = match &action {
                    Action::Insert | Action::Update { .. } => true,
                    Action::Keep { stored_hash, .. } => verify_all || stored_hash.is_none(),
                };
                Planned { file, action, needs_hash }
            })
            .collect();
        let missing: Vec<idx::IndexedAsset> = by_path.into_values().collect();

        // 2. Hash outside the transaction so slow disks do not hold the SQLite write lock.
        let to_hash: Vec<(usize, std::path::PathBuf)> = plan
            .iter()
            .enumerate()
            .filter(|(_, p)| p.needs_hash)
            .map(|(i, p)| (i, project_dir.join(&p.file.rel_path)))
            .collect();
        let mut hashes: HashMap<usize, String> = HashMap::with_capacity(to_hash.len());
        if !to_hash.is_empty() {
            let results = tokio::task::spawn_blocking(move || {
                to_hash
                    .into_iter()
                    .map(|(i, path)| (i, crate::hash::hash_file(&path).map_err(|e| e.to_string())))
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(|e| ScanError::Io(e.to_string()))?;

            for (i, result) in results {
                match result {
                    Ok(hash) => {
                        self.stats.assets_hashed += 1;
                        hashes.insert(i, hash);
                    }
                    Err(e) => self.skip_event(&format!("{folder}/{}", plan[i].file.rel_path), &format!("Hashing failed: {e}")).await,
                }
            }
        }

        // 3. Apply.
        let now = now();
        let mut tx = self.pool.begin().await?;

        let (project_id, mut main_image_id, is_new) = match existing {
            Some(p) => (p.id.clone(), p.main_image_id.clone(), false),
            None => (idx::insert_discovered_project(&mut tx, folder, &now).await?, None, true),
        };

        let mut changes = ProjectChanges::default();
        let mut content_changed: Vec<&str> = Vec::new();
        let mut first_image: Option<String> = None;
        let mut thumb_jobs: Vec<ThumbJob> = Vec::new();

        for (i, planned) in plan.iter().enumerate() {
            let file = planned.file;
            let hash = hashes.get(&i).map(String::as_str);

            let mut force_thumb = true;
            let asset_id = match &planned.action {
                Action::Insert => {
                    changes.added += 1;
                    insert_asset(&mut tx, &project_id, file, hash, &now).await?
                }
                Action::Update { asset_id } => {
                    idx::update_changed_asset(&mut tx, asset_id, file.size, &file.mtime, hash, &now).await?;
                    changes.updated += 1;
                    asset_id.clone()
                }
                Action::Keep { asset_id, stored_hash } => {
                    match (stored_hash.as_deref(), hash) {
                        // Same size and mtime but different bytes: content was rewritten in place.
                        (Some(stored), Some(new)) if stored != new => {
                            idx::update_changed_asset(&mut tx, asset_id, file.size, &file.mtime, Some(new), &now).await?;
                            changes.updated += 1;
                            content_changed.push(&file.rel_path);
                        }
                        (None, Some(new)) => {
                            idx::set_asset_hash(&mut tx, asset_id, new).await?;
                            force_thumb = false;
                        }
                        _ => force_thumb = false,
                    }
                    asset_id.clone()
                }
            };

            if asset_kind(&file.rel_path) == "image" {
                if first_image.is_none() {
                    first_image = Some(asset_id.clone());
                }
                thumb_jobs.push(ThumbJob { asset_id, source: project_dir.join(&file.rel_path), force: force_thumb });
            }
        }

        for asset in &missing {
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

        for asset in &missing {
            self.remove_thumbs(&project_id, Some(&asset.id)).await;
        }
        self.generate_thumbs(&project_id, thumb_jobs);

        self.stats.assets_added += changes.added;
        self.stats.assets_updated += changes.updated;
        self.stats.assets_removed += changes.removed;

        for path in content_changed {
            self.event(
                "warn",
                "asset_content_changed",
                json!({ "project_id": project_id, "path": format!("{folder}/{path}"), "detail": "Content hash changed while size and mtime stayed the same" }),
            )
            .await;
        }

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

    /// Queues missing or stale thumbnails on the background workers.
    fn generate_thumbs(&mut self, project_id: &str, jobs: Vec<ThumbJob>) {
        for job in jobs {
            let dest = thumbnails::thumb_path(self.thumbs_root, project_id, &job.asset_id);
            let permits = self.thumb_permits.clone();
            self.thumb_tasks.spawn(async move {
                let _permit = permits.acquire_owned().await;
                let source = job.source.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if !job.force && dest.exists() {
                        return None;
                    }
                    Some(thumbnails::generate(&job.source, &dest))
                })
                .await
                .unwrap_or_else(|e| Some(Err(format!("Thumbnail task crashed: {e}"))));
                (source, result)
            });
        }
    }

    /// Records finished thumbnail jobs. With `wait` blocks until all are done.
    async fn drain_thumbs(&mut self, wait: bool) {
        loop {
            let next = if wait { self.thumb_tasks.join_next().await } else { self.thumb_tasks.try_join_next() };
            let Some(joined) = next else { break };
            match joined {
                Ok((_, None)) => {}
                Ok((_, Some(Ok(())))) => self.stats.thumbnails_generated += 1,
                Ok((source, Some(Err(e)))) => {
                    self.stats.errors += 1;
                    let path = source.strip_prefix(self.root).unwrap_or(&source).display().to_string();
                    self.event("warn", "thumbnail_failed", json!({ "path": path, "error": e })).await;
                }
                Err(e) => {
                    self.stats.errors += 1;
                    self.event("error", "thumbnail_failed", json!({ "error": e.to_string() })).await;
                }
            }
        }
        if wait {
            self.save_stats().await;
        }
    }

    /// Removes one asset thumbnail, or the whole project folder when `asset_id` is None.
    async fn remove_thumbs(&self, project_id: &str, asset_id: Option<&str>) {
        let root = self.thumbs_root.to_path_buf();
        let project = project_id.to_string();
        let asset = asset_id.map(str::to_string);
        let result = tokio::task::spawn_blocking(move || match asset {
            Some(asset) => thumbnails::remove_asset(&root, &project, &asset),
            None => thumbnails::remove_project(&root, &project),
        })
        .await;
        if let Ok(Err(e)) = result {
            tracing::warn!("Failed to remove thumbnails for project {project_id}: {e}");
        }
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
    file_hash: Option<&str>,
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
        file_hash,
        now,
    )
    .await
}
