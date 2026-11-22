//! Sync run orchestration. Only one run executes at a time; each run is recorded
//! in `sync_runs` and its notable actions in `sync_events`.

mod fs_walk;
mod scan;
mod watcher;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use lima_domain::models::sync::{SyncOptions, SyncRunRow, SyncStats, WatcherStatus};
use sqlx::{Pool, Sqlite};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex, Notify};

pub use watcher::WatcherConfig;

pub use scan::ScanError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Full,
    Incremental,
}

impl SyncMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::Full => "full",
            SyncMode::Incremental => "incremental",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncReason {
    Manual,
    Scheduled,
    Watcher,
}

impl SyncReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncReason::Manual => "manual",
            SyncReason::Scheduled => "scheduled",
            SyncReason::Watcher => "watcher",
        }
    }
}

#[derive(Debug)]
pub enum StartSyncError {
    AlreadyRunning { run_id: String },
    Db(sqlx::Error),
}

impl std::fmt::Display for StartSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartSyncError::AlreadyRunning { run_id } => write!(f, "Sync run {run_id} is already in progress"),
            StartSyncError::Db(e) => write!(f, "Database error: {e}"),
        }
    }
}

/// Cheap to clone handle shared through the server state.
#[derive(Clone)]
pub struct SyncManager {
    inner: Arc<Inner>,
}

struct Inner {
    pool: Pool<Sqlite>,
    library_root: PathBuf,
    thumbs_root: PathBuf,
    /// Id of the run currently executing, if any.
    current: Mutex<Option<String>>,
    /// Signalled whenever a run ends.
    run_finished: Notify,
    watcher_status: std::sync::Mutex<WatcherStatus>,
    /// Keeps the OS watcher alive; dropping it stops events.
    watcher: std::sync::Mutex<Option<notify::RecommendedWatcher>>,
}

impl SyncManager {
    /// Creates the manager and marks runs left over from a previous process as failed.
    pub async fn new(
        pool: Pool<Sqlite>,
        library_root: impl Into<PathBuf>,
        thumbs_root: impl Into<PathBuf>,
    ) -> Result<Self, sqlx::Error> {
        let interrupted = lima_db::queries::sync::fail_interrupted_runs(&pool, &now()).await?;
        if interrupted > 0 {
            tracing::warn!("Marked {interrupted} interrupted sync run(s) as failed");
        }

        Ok(Self {
            inner: Arc::new(Inner {
                pool,
                library_root: library_root.into(),
                thumbs_root: thumbs_root.into(),
                current: Mutex::new(None),
                run_finished: Notify::new(),
                watcher_status: std::sync::Mutex::new(WatcherStatus::default()),
                watcher: std::sync::Mutex::new(None),
            }),
        })
    }

    /// Id of the run in progress, if any.
    pub async fn current_run_id(&self) -> Option<String> {
        self.inner.current.lock().await.clone()
    }

    /// Records a new run over the whole library and executes it in the background.
    pub async fn start(&self, mode: SyncMode, reason: SyncReason) -> Result<SyncRunRow, StartSyncError> {
        self.start_scoped(mode, reason, None).await
    }

    /// Like [`start`](Self::start) but limited to some project folders when `folders` is set.
    pub async fn start_scoped(
        &self,
        mode: SyncMode,
        reason: SyncReason,
        folders: Option<Vec<String>>,
    ) -> Result<SyncRunRow, StartSyncError> {
        let mut current = self.inner.current.lock().await;
        if let Some(run_id) = current.as_ref() {
            return Err(StartSyncError::AlreadyRunning { run_id: run_id.clone() });
        }

        let run_id = uuid::Uuid::new_v4().to_string();
        let run = lima_db::queries::sync::create_run(
            &self.inner.pool,
            &run_id,
            mode.as_str(),
            reason.as_str(),
            &SyncOptions { folders: folders.clone() },
            &now(),
        )
        .await
        .map_err(StartSyncError::Db)?;

        *current = Some(run_id.clone());
        drop(current);

        tracing::info!("Sync run {run_id} started (mode={}, reason={})", mode.as_str(), reason.as_str());

        let scope: Option<HashSet<String>> = folders.map(|f| f.into_iter().collect());
        let inner = self.inner.clone();
        tokio::spawn(async move {
            // Run the scan in its own task so a panic is reported as a failed run.
            let scan_inner = inner.clone();
            let scan_run_id = run_id.clone();
            let outcome = tokio::spawn(async move {
                scan::Scanner::new(
                    &scan_inner.pool,
                    &scan_inner.library_root,
                    &scan_inner.thumbs_root,
                    &scan_run_id,
                    mode,
                    scope,
                )
                    .run()
                    .await
            })
            .await;

            let (status, stats, error) = match outcome {
                Ok((stats, Ok(()))) => ("complete", stats, String::new()),
                Ok((stats, Err(e))) => ("failed", stats, e.to_string()),
                Err(join_err) => ("failed", SyncStats::default(), format!("Sync task crashed: {join_err}")),
            };

            if let Err(e) = lima_db::queries::sync::finish_run(&inner.pool, &run_id, status, &stats, &error, &now()).await {
                tracing::error!("Failed to record end of sync run {run_id}: {e}");
            }

            match status {
                "complete" => tracing::info!("Sync run {run_id} complete: {stats:?}"),
                _ => tracing::error!("Sync run {run_id} failed: {error}"),
            }

            *inner.current.lock().await = None;
            inner.run_finished.notify_waiters();
        });

        Ok(run)
    }

    /// Waits until the run in progress (if any) ends.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.inner.run_finished.notified();
            tokio::pin!(notified);
            // Register before checking so a run ending in between is not missed.
            notified.as_mut().enable();
            if self.current_run_id().await.is_none() {
                return;
            }
            notified.await;
        }
    }

    pub fn watcher_status(&self) -> WatcherStatus {
        self.inner.watcher_status.lock().unwrap().clone()
    }

    pub(crate) fn update_watcher_status(&self, f: impl FnOnce(&mut WatcherStatus)) {
        f(&mut self.inner.watcher_status.lock().unwrap());
    }

    pub fn library_root(&self) -> &std::path::Path {
        &self.inner.library_root
    }

    /// Starts watching the library folder and triggering incremental syncs for
    /// changed project folders. Best effort: failures are reported through
    /// [`watcher_status`](Self::watcher_status), manual syncs keep working.
    pub fn start_watcher(&self, config: WatcherConfig) {
        watcher::spawn(self.clone(), config);
    }

    pub(crate) fn set_os_watcher(&self, w: Option<notify::RecommendedWatcher>) {
        *self.inner.watcher.lock().unwrap() = w;
    }
}

pub(crate) fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

#[cfg(test)]
mod tests;
