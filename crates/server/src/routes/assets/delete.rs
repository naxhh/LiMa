use axum::{extract::{State, Path}, http::StatusCode};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};
use lima_db::queries::assets::DeleteAssetError;

#[utoipa::path(
    delete,
    path = "/api/projects/{project_id}/assets/{asset_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project containing the asset"),
        ("asset_id" = String, Path, description = "The ID of the asset to delete"),
    ),
    responses(
        (status = 200, description = "Asset deleted successfully"),
        (status = 404, description = "Asset not found in the specified project", body = ApiErrorBody),
        (status = 500, description = "Failed to delete asset", body = ApiErrorBody),
    )
)]
pub async fn asset_delete(
    State(_state): State<AppState>,
    Path((project_id, asset_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiErrorResponse> {
    match lima_db::queries::assets::delete_asset(
        _state.db.pool(),
        &project_id,
        &asset_id,
    ).await {
        Ok(_) => Ok(StatusCode::OK),
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
