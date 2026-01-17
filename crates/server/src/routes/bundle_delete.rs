use axum::{ http::StatusCode, extract::{Path, State} };
use tokio::fs;
use std::{path::PathBuf};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[utoipa::path(
    delete,
    path = "/bundles/{bundle_id}",
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
    State(_state): State<AppState>,
    Path(bundle_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {
    // TODO: extract these paths to a common place. State? domain config?
    let bundle_folder: PathBuf = ["data", "state", "bundles", &bundle_id].iter().collect();

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