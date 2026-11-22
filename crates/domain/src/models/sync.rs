use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// A recorded synchronization run between the library folder and the database.
#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct SyncRunRow {
    pub id: String,
    /// `full` or `incremental`
    pub mode: String,
    /// `manual`, `scheduled` or `watcher`
    pub reason: String,
    /// `queued`, `running`, `complete`, `failed` or `cancelled`
    pub status: String,
    #[sqlx(json, rename = "stats_json")]
    pub stats: SyncStats,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: String,
}

/// Counters for a sync run. Updated while the run progresses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct SyncStats {
    /// Number of project folders found on disk (known once discovery finishes).
    pub projects_total: u64,
    pub projects_scanned: u64,
    pub projects_added: u64,
    pub projects_removed: u64,
    pub assets_added: u64,
    pub assets_updated: u64,
    pub assets_removed: u64,
    /// Files whose content was hashed during the run.
    pub assets_hashed: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct SyncEventRow {
    pub id: i64,
    pub run_id: String,
    pub created_at: String,
    /// `trace`, `debug`, `info`, `warn` or `error`
    pub level: String,
    pub kind: String,
    #[schema(value_type = Object)]
    #[sqlx(json, rename = "data_json")]
    pub data: serde_json::Value,
}
