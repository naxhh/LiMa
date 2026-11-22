use axum::{ http::StatusCode, extract::{Path, State} };
use tokio::fs;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[utoipa::path(
    delete,
    path = "/api/bundles/{bundle_id}",
    params(
        ("bundle_id" = String, Path, description = "The ID of the bundle to delete"),
    ),
    responses(
        (status = 200, description = "Bundle deleted successfully"),
        (status = 404, description = "Bundle not found", body = ApiErrorBody),
        (status = 500, description = "Failed to delete bundle", body = ApiErrorBody),
    )
)]
pub async fn bundle_delete(
    State(state): State<AppState>,
    Path(bundle_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {
    let Some(bundle_folder) = state.paths.bundle(&bundle_id) else {
        return Err(ApiErrorResponse::new(
            StatusCode::NOT_FOUND,
            "bundle_not_found",
            "The specified bundle does not exist",
        ));
    };

    match fs::remove_dir_all(&bundle_folder).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!("Bundle not found for deletion: {}", bundle_id);
            Err(ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "bundle_not_found",
                "The specified bundle does not exist",
            ))
        }
        Err(e) => {
            tracing::error!("Failed to delete bundle {}: {}", bundle_id, e);
            Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "deletion_failed",
                "Failed to delete the specified bundle",
            ).with_cause(&e.to_string()))
        }
    }
}