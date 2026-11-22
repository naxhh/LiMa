//! Folders removed from LiMa with their files kept. Sync skips them until un-ignored.

use axum::{extract::{Query, State}, http::StatusCode, Json};
use lima_indexer::sync::{SyncMode, SyncReason};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct IgnoredFolderItem {
    /// Folder path relative to the library root.
    pub folder_path: String,
    /// When it was removed from LiMa (RFC 3339).
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct ListIgnoredFoldersResponse {
    pub items: Vec<IgnoredFolderItem>,
}

#[derive(Deserialize, IntoParams)]
pub struct UnignoreParams {
    /// Folder path relative to the library root.
    pub folder_path: String,
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

#[utoipa::path(
    get,
    path = "/api/ignored-folders",
    tag = "Sync",
    responses(
        (status = 200, description = "Folders that sync skips", body = ListIgnoredFoldersResponse),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_ignored_folders(State(state): State<AppState>) -> Result<Json<ListIgnoredFoldersResponse>, ApiErrorResponse> {
    let rows = lima_db::queries::ignored_folders::list_ignored(state.db.pool()).await.map_err(db_error)?;
    Ok(Json(ListIgnoredFoldersResponse {
        items: rows
            .into_iter()
            .map(|r| IgnoredFolderItem { folder_path: r.folder_path, created_at: r.created_at })
            .collect(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/ignored-folders",
    tag = "Sync",
    params(UnignoreParams),
    responses(
        (status = 204, description = "Un-ignored; a sync of that folder starts right away (or it's picked up by the next sync if one is running)"),
        (status = 404, description = "The folder wasn't ignored", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn unignore_folder(
    State(state): State<AppState>,
    Query(params): Query<UnignoreParams>,
) -> Result<StatusCode, ApiErrorResponse> {
    let removed = lima_db::queries::ignored_folders::unignore(state.db.pool(), &params.folder_path).await.map_err(db_error)?;
    if !removed {
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "not_ignored", "That folder isn't ignored"));
    }
    // Bring it back now. If a sync is already running, the next one picks it up.
    if let Err(e) = state
        .sync
        .start_scoped(SyncMode::Incremental, SyncReason::Manual, Some(vec![params.folder_path.clone()]))
        .await
    {
        tracing::info!("Un-ignored {} but couldn't sync it now: {e:?}", params.folder_path);
    }
    Ok(StatusCode::NO_CONTENT)
}
