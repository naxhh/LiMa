use axum::{extract::{State, Path}, http::StatusCode, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};
use lima_db::queries::assets::DeleteAssetError;

#[derive(Serialize, ToSchema)]
pub struct DeleteAssetResponse {
    /// The trash item to restore the file from; none when the trash is off.
    pub trash_id: Option<String>,
}

#[utoipa::path(
    delete,
    path = "/api/projects/{project_id}/assets/{asset_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project containing the asset"),
        ("asset_id" = String, Path, description = "The ID of the asset to delete"),
    ),
    responses(
        (status = 200, description = "Deleted: moved to the trash when it's on (restore it with `trash_id`), else removed from disk", body = DeleteAssetResponse),
        (status = 404, description = "Asset not found in the specified project", body = ApiErrorBody),
        (status = 500, description = "Failed to delete asset", body = ApiErrorBody),
    )
)]
pub async fn asset_delete(
    State(_state): State<AppState>,
    Path((project_id, asset_id)): Path<(String, String)>,
) -> Result<Json<DeleteAssetResponse>, ApiErrorResponse> {
    if _state.trash_days > 0 {
        return match crate::trash::trash_file(&_state, &project_id, &asset_id).await {
            Ok(trash_id) => Ok(Json(DeleteAssetResponse { trash_id: Some(trash_id) })),
            Err(crate::trash::TrashError::NotFound) => {
                Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "asset_not_found", "Asset not found in the specified project"))
            }
            Err(crate::trash::TrashError::Db(e)) => {
                Err(ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "database_error", "Database error occurred").with_cause(&e.to_string()))
            }
            Err(crate::trash::TrashError::Io(e)) => {
                Err(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "filesystem_error", "Failed to move the file to the trash").with_cause(&e))
            }
        };
    }
    match lima_db::queries::assets::delete_asset(
        _state.db.pool(),
        &_state.paths,
        &project_id,
        &asset_id,
    ).await {
        Ok(_) => {
            let thumbs = _state.paths.thumbnails();
            let (project, asset) = (project_id.clone(), asset_id.clone());
            let removed = tokio::task::spawn_blocking(move || lima_indexer::thumbnails::remove_asset(&thumbs, &project, &asset)).await;
            if let Ok(Err(e)) = removed {
                tracing::warn!("Failed to remove thumbnail of asset {asset_id}: {e}");
            }
            Ok(Json(DeleteAssetResponse { trash_id: None }))
        }
        Err(DeleteAssetError::NotFound { project_id: _ }) => {
            return Err(ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "asset_not_found",
                "Asset not found in the specified project",
            ));
        }
        Err(DeleteAssetError::Db(e)) => {
            return Err(ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                &"Database error occurred",
            ).with_cause(&e.to_string()));
        }
        Err(DeleteAssetError::Fs(e)) => {
            return Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "filesystem_error",
                "Failed to delete asset file from disk",
            ).with_cause(&e.to_string()));
        }
    }

}
