//! The Sync page's view of collection syncs: which collections are synced and when they're
//! due, each run's result, and one history mixing library and collection runs.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use lima_db::queries::collection_sources::{self as db, RunRow};
use lima_domain::models::sync::SyncRunRow;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use utoipa::{IntoParams, ToSchema};

use crate::collection_sync::{self, CollectionJob, FailedModel};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct CollectionSyncItem {
    pub collection_id: String,
    pub name: String,
    pub source: String,
    pub url: String,
    pub keep_in_sync: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    /// When the next automatic sync is due (RFC 3339); none when not kept in sync or when
    /// automatic syncs are off.
    pub next_sync_at: Option<String>,
    /// The run in progress, if any.
    pub job_id: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionSyncsResponse {
    /// Hours between automatic syncs (0 = only by hand).
    pub interval_hours: u64,
    /// Synced collections first, soonest due first; then the others by name.
    pub items: Vec<CollectionSyncItem>,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionSyncRun {
    pub id: String,
    pub collection_id: String,
    pub collection_name: String,
    pub source: String,
    /// `import`, `manual` or `scheduled`.
    pub reason: String,
    /// `running`, `complete` or `failed`.
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub imported: i64,
    pub linked: i64,
    pub removed: i64,
    pub failed: Vec<FailedModel>,
    pub error: Option<String>,
    /// Live progress while running (or finished in the last hour).
    pub progress: Option<CollectionJob>,
}

#[derive(Serialize, ToSchema)]
pub struct SyncHistoryItem {
    /// `library` or `collection`.
    pub kind: String,
    pub id: String,
    pub started_at: Option<String>,
    pub library: Option<SyncRunRow>,
    pub collection: Option<CollectionSyncRun>,
}

#[derive(Serialize, ToSchema)]
pub struct SyncHistoryResponse {
    pub items: Vec<SyncHistoryItem>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct HistoryParams {
    pub limit: Option<i64>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

fn run_response(state: &AppState, r: RunRow) -> CollectionSyncRun {
    CollectionSyncRun {
        failed: serde_json::from_str(&r.failures_json).unwrap_or_default(),
        progress: if r.status == "running" { collection_sync::job(state, &r.id) } else { None },
        id: r.id,
        collection_id: r.collection_id,
        collection_name: r.collection_name,
        source: r.source,
        reason: r.reason,
        status: r.status,
        started_at: r.started_at,
        finished_at: r.finished_at,
        imported: r.imported,
        linked: r.linked,
        removed: r.removed,
        error: r.error,
    }
}

#[utoipa::path(
    get,
    path = "/api/collection-syncs",
    tag = "Sync",
    responses((status = 200, description = "Imported collections and when they sync next", body = CollectionSyncsResponse))
)]
pub async fn list_collection_syncs(State(state): State<AppState>) -> Result<Json<CollectionSyncsResponse>, ApiErrorResponse> {
    let hours = state.collection_sync_hours;
    let mut rows = db::overview(state.db.pool()).await.map_err(db_error)?;
    rows.sort_by(|a, b| {
        let due = |r: &db::OverviewRow| (r.keep_in_sync && hours > 0).then(|| collection_sync::next_sync_at(r.last_synced_at.as_deref(), hours));
        match (due(a), due(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    let items = rows
        .into_iter()
        .map(|r| CollectionSyncItem {
            next_sync_at: (r.keep_in_sync && hours > 0)
                .then(|| collection_sync::next_sync_at(r.last_synced_at.as_deref(), hours).format(&Rfc3339).unwrap_or_default()),
            job_id: collection_sync::running_job(&state, &r.collection_id),
            collection_id: r.collection_id,
            name: r.name,
            source: r.source,
            url: r.url,
            keep_in_sync: r.keep_in_sync,
            last_synced_at: r.last_synced_at,
            last_error: r.last_error,
        })
        .collect();
    Ok(Json(CollectionSyncsResponse { interval_hours: hours, items }))
}

#[utoipa::path(
    get,
    path = "/api/collection-syncs/runs/{run_id}",
    tag = "Sync",
    params(("run_id" = String, Path)),
    responses(
        (status = 200, description = "One collection import or sync", body = CollectionSyncRun),
        (status = 404, description = "Unknown run", body = ApiErrorBody),
    )
)]
pub async fn get_collection_sync_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Result<Json<CollectionSyncRun>, ApiErrorResponse> {
    let r = db::get_run(state.db.pool(), &run_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "run_not_found", "Unknown run"))?;
    Ok(Json(run_response(&state, r)))
}

#[utoipa::path(
    get,
    path = "/api/sync/history",
    tag = "Sync",
    params(HistoryParams),
    responses((status = 200, description = "Library and collection runs, newest first", body = SyncHistoryResponse))
)]
pub async fn sync_history(State(state): State<AppState>, Query(params): Query<HistoryParams>) -> Result<Json<SyncHistoryResponse>, ApiErrorResponse> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let before = params.cursor.as_deref().and_then(|c| c.split_once('|')).map(|(t, id)| (t.to_string(), id.to_string()));
    let pool = state.db.pool();

    // Both lists filtered by the same cursor, merged, cut to `limit`.
    let library = lima_db::queries::sync::list_runs(
        pool,
        limit,
        before.as_ref().map(|(t, id)| lima_domain::pagination::Cursor { updated_at: t.clone(), id: id.clone(), rank: None, sort: None, sort_value: None }),
    )
    .await
    .map_err(db_error)?;
    let collections = db::list_runs(pool, limit, before.as_ref().map(|(t, id)| (t.as_str(), id.as_str()))).await.map_err(db_error)?;

    let mut items: Vec<SyncHistoryItem> = library
        .into_iter()
        .map(|r| SyncHistoryItem { kind: "library".into(), id: r.id.clone(), started_at: r.started_at.clone(), library: Some(r), collection: None })
        .chain(collections.into_iter().map(|r| SyncHistoryItem {
            kind: "collection".into(),
            id: r.id.clone(),
            started_at: Some(r.started_at.clone()),
            library: None,
            collection: Some(run_response(&state, r)),
        }))
        .collect();
    items.sort_by(|a, b| (b.started_at.as_deref().unwrap_or(""), &b.id).cmp(&(a.started_at.as_deref().unwrap_or(""), &a.id)));
    items.truncate(limit as usize);
    let next_cursor = (items.len() as i64 == limit).then(|| items.last().map(|i| format!("{}|{}", i.started_at.as_deref().unwrap_or(""), i.id))).flatten();
    Ok(Json(SyncHistoryResponse { items, next_cursor }))
}
