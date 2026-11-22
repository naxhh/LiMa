use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use lima_domain::models::sync::{SyncEventRow, SyncRunRow};
use lima_domain::pagination::{Cursor, decode_cursor, encode_cursor};
use lima_indexer::sync::{StartSyncError, SyncMode, SyncReason};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

fn db_error(message: &'static str, e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", message).with_cause(&e.to_string())
}

#[derive(Deserialize, ToSchema, Default)]
pub struct StartSyncRequest {
    /// `incremental` (default) only hashes new, changed or unhashed files.
    /// `full` also re-hashes every file to detect in-place content changes.
    pub mode: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/sync/run",
    tag = "Sync",
    request_body(content = Option<StartSyncRequest>, description = "Optional run options"),
    responses(
        (status = 202, description = "Sync run started", body = SyncRunRow),
        (status = 400, description = "Invalid mode", body = ApiErrorBody),
        (status = 409, description = "A sync run is already in progress", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn start_sync(
    State(state): State<AppState>,
    body: Option<Json<StartSyncRequest>>,
) -> Result<(StatusCode, Json<SyncRunRow>), ApiErrorResponse> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let mode = match body.mode.as_deref().unwrap_or("incremental") {
        "full" => SyncMode::Full,
        "incremental" => SyncMode::Incremental,
        _ => {
            return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_mode", "Mode must be 'full' or 'incremental'"));
        }
    };

    match state.sync.start(mode, SyncReason::Manual).await {
        Ok(run) => Ok((StatusCode::ACCEPTED, Json(run))),
        Err(StartSyncError::AlreadyRunning { run_id }) => Err(ApiErrorResponse::new(
            StatusCode::CONFLICT,
            "sync_already_running",
            "A sync run is already in progress",
        )
        .with_details(serde_json::json!({ "run_id": run_id }))),
        Err(StartSyncError::Db(e)) => Err(db_error("Failed to start sync run", e)),
    }
}

#[derive(Serialize, ToSchema)]
pub struct SyncStatusResponse {
    /// True while a run is executing.
    pub running: bool,
    /// The run in progress, or the most recent one.
    pub run: Option<SyncRunRow>,
}

#[utoipa::path(
    get,
    path = "/api/sync/status",
    tag = "Sync",
    responses(
        (status = 200, description = "Current or last sync run", body = SyncStatusResponse),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn sync_status(State(state): State<AppState>) -> Result<Json<SyncStatusResponse>, ApiErrorResponse> {
    let pool = state.db.pool();
    let run = match state.sync.current_run_id().await {
        Some(id) => lima_db::queries::sync::get_run(pool, &id).await,
        None => lima_db::queries::sync::latest_run(pool).await,
    }
    .map_err(|e| db_error("Failed to load sync status", e))?;

    Ok(Json(SyncStatusResponse {
        running: run.as_ref().is_some_and(|r| r.status == "running"),
        run,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ListSyncRunsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ListSyncRunsResponse {
    pub items: Vec<SyncRunRow>,
    pub next_cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/sync/runs",
    tag = "Sync",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum runs to return (default: 20, max: 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
    ),
    responses(
        (status = 200, description = "Sync runs, newest first", body = ListSyncRunsResponse),
        (status = 400, description = "Invalid cursor", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_sync_runs(
    State(state): State<AppState>,
    Query(params): Query<ListSyncRunsParams>,
) -> Result<Json<ListSyncRunsResponse>, ApiErrorResponse> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let cursor = params
        .cursor
        .as_deref()
        .map(decode_cursor)
        .transpose()
        .map_err(|e| ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Invalid cursor parameter").with_cause(&e))?;

    let items = lima_db::queries::sync::list_runs(state.db.pool(), limit, cursor)
        .await
        .map_err(|e| db_error("Failed to list sync runs", e))?;

    let next_cursor = if items.len() as i64 == limit {
        items.last().map(|r| {
            encode_cursor(&Cursor { updated_at: r.started_at.clone().unwrap_or_default(), id: r.id.clone(), rank: None })
        })
    } else {
        None
    };

    Ok(Json(ListSyncRunsResponse { items, next_cursor }))
}

#[utoipa::path(
    get,
    path = "/api/sync/runs/{run_id}",
    tag = "Sync",
    params(("run_id" = String, Path, description = "Sync run id")),
    responses(
        (status = 200, description = "Sync run", body = SyncRunRow),
        (status = 404, description = "Run not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn get_sync_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<SyncRunRow>, ApiErrorResponse> {
    lima_db::queries::sync::get_run(state.db.pool(), &run_id)
        .await
        .map_err(|e| db_error("Failed to load sync run", e))?
        .map(Json)
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "sync_run_not_found", "Sync run not found"))
}

#[derive(Deserialize, ToSchema)]
pub struct ListSyncEventsParams {
    /// Return events with an id greater than this (exclusive). Use the last seen id to tail a run.
    pub after: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ListSyncEventsResponse {
    pub items: Vec<SyncEventRow>,
    /// Pass as `after` to fetch the next page. Null when there are no more events right now.
    pub next_after: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/sync/runs/{run_id}/events",
    tag = "Sync",
    params(
        ("run_id" = String, Path, description = "Sync run id"),
        ("after" = Option<i64>, Query, description = "Only events with id greater than this"),
        ("limit" = Option<i64>, Query, description = "Maximum events to return (default: 200, max: 1000)"),
    ),
    responses(
        (status = 200, description = "Sync events, oldest first", body = ListSyncEventsResponse),
        (status = 404, description = "Run not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_sync_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(params): Query<ListSyncEventsParams>,
) -> Result<Json<ListSyncEventsResponse>, ApiErrorResponse> {
    let pool = state.db.pool();
    let limit = params.limit.unwrap_or(200).clamp(1, 1000);

    if lima_db::queries::sync::get_run(pool, &run_id).await.map_err(|e| db_error("Failed to load sync run", e))?.is_none() {
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "sync_run_not_found", "Sync run not found"));
    }

    let items = lima_db::queries::sync::list_events(pool, &run_id, params.after.unwrap_or(0), limit)
        .await
        .map_err(|e| db_error("Failed to list sync events", e))?;

    let next_after = if items.len() as i64 == limit { items.last().map(|e| e.id) } else { None };

    Ok(Json(ListSyncEventsResponse { items, next_after }))
}
