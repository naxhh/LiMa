//! The trash: deleted projects that can still be restored.

use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Serialize;
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;
use crate::trash::TrashError;

#[derive(Serialize, ToSchema)]
pub struct TrashItem {
    pub id: String,
    /// Where the project was, relative to the library root.
    pub folder_path: String,
    pub name: String,
    pub file_count: i64,
    pub size_bytes: i64,
    pub trashed_at: String,
    /// When it will be deleted for good (RFC 3339).
    pub expires_at: String,
    /// Set when this is one file (relative to `folder_path`) rather than a whole project.
    pub file_path: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ListTrashResponse {
    /// Days items are kept; 0 means the trash is disabled and deletes are final.
    pub retention_days: u64,
    pub items: Vec<TrashItem>,
}

#[derive(Serialize, ToSchema)]
pub struct RestoreTrashResponse {
    /// The restored project.
    pub project_id: String,
}

fn error(e: TrashError) -> ApiErrorResponse {
    match e {
        TrashError::NotFound => ApiErrorResponse::new(StatusCode::NOT_FOUND, "trash_item_not_found", "Not in the trash"),
        TrashError::Io(msg) => ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "trash_failed", "Trash operation failed").with_cause(&msg),
        TrashError::Db(e) => ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string()),
    }
}

fn expires(trashed_at: &str, days: u64) -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(trashed_at, &Rfc3339)
        .map(|t| (t + time::Duration::days(days as i64)).format(&Rfc3339).unwrap_or_default())
        .unwrap_or_default()
}

#[utoipa::path(
    get,
    path = "/api/trash",
    tag = "Trash",
    responses(
        (status = 200, description = "Trashed projects, newest first", body = ListTrashResponse),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_trash(State(state): State<AppState>) -> Result<Json<ListTrashResponse>, ApiErrorResponse> {
    let rows = lima_db::queries::trash::list(state.db.pool()).await.map_err(|e| error(e.into()))?;
    Ok(Json(ListTrashResponse {
        retention_days: state.trash_days,
        items: rows
            .into_iter()
            .map(|r| TrashItem {
                expires_at: expires(&r.trashed_at, state.trash_days),
                id: r.id,
                folder_path: r.folder_path,
                name: r.name,
                file_count: r.file_count,
                size_bytes: r.size_bytes,
                trashed_at: r.trashed_at,
                file_path: r.file_path,
            })
            .collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/trash/{trash_id}/restore",
    tag = "Trash",
    params(("trash_id" = String, Path, description = "Trash item id")),
    responses(
        (status = 200, description = "Restored with its tags, collections and description. If its folder name was taken, it comes back as `<name>-restored`.", body = RestoreTrashResponse),
        (status = 404, description = "Not in the trash", body = ApiErrorBody),
        (status = 500, description = "Moving the folder back failed", body = ApiErrorBody),
    )
)]
pub async fn restore_trash(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<RestoreTrashResponse>, ApiErrorResponse> {
    let project_id = crate::trash::restore(&state, &id).await.map_err(error)?;
    Ok(Json(RestoreTrashResponse { project_id }))
}

#[utoipa::path(
    delete,
    path = "/api/trash/{trash_id}",
    tag = "Trash",
    params(("trash_id" = String, Path, description = "Trash item id")),
    responses(
        (status = 204, description = "Deleted for good"),
        (status = 404, description = "Not in the trash", body = ApiErrorBody),
    )
)]
pub async fn delete_trash_item(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiErrorResponse> {
    let row = lima_db::queries::trash::get(state.db.pool(), &id).await.map_err(|e| error(e.into()))?.ok_or_else(|| error(TrashError::NotFound))?;
    crate::trash::purge(&state, &row).await.map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/trash",
    tag = "Trash",
    responses((status = 204, description = "Trash emptied"))
)]
pub async fn empty_trash(State(state): State<AppState>) -> Result<StatusCode, ApiErrorResponse> {
    for row in lima_db::queries::trash::list(state.db.pool()).await.map_err(|e| error(e.into()))? {
        crate::trash::purge(&state, &row).await.map_err(error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}
