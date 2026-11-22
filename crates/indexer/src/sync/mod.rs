//! Sync run orchestration. Only one run executes at a time; each run is recorded
//! in `sync_runs` and its notable actions in `sync_events`.

mod fs_walk;
mod scan;

use std::path::PathBuf;
use std::sync::Arc;

use lima_domain::models::sync::{SyncRunRow, SyncStats};
use sqlx::{Pool, Sqlite};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;

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
            }),
        })
    }

    /// Id of the run in progress, if any.
    pub async fn current_run_id(&self) -> Option<String> {
        self.inner.current.lock().await.clone()
    }

    /// Records a new run and executes it in the background. Returns the freshly created run.
    pub async fn start(&self, mode: SyncMode, reason: SyncReason) -> Result<SyncRunRow, StartSyncError> {
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
            &now(),
        )
        .await
        .map_err(StartSyncError::Db)?;

        *current = Some(run_id.clone());
        drop(current);

        tracing::info!("Sync run {run_id} started (mode={}, reason={})", mode.as_str(), reason.as_str());

        let inner = self.inner.clone();
        tokio::spawn(async move {
            // Run the scan in its own task so a panic is reported as a failed run.
            let scan_inner = inner.clone();
            let scan_run_id = run_id.clone();
            let outcome = tokio::spawn(async move {
                scan::Scanner::new(&scan_inner.pool, &scan_inner.library_root, &scan_inner.thumbs_root, &scan_run_id, mode)
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
        });

        Ok(run)
    }
}

pub(crate) fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

#[cfg(test)]
mod tests;
