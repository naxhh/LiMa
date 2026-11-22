use axum::{ http::StatusCode, extract::{Path, Query, State} };
use serde::Deserialize;
use utoipa::IntoParams;
use lima_db::queries::projects_delete::DeleteProjectError;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};


#[derive(Deserialize, IntoParams)]
pub struct DeleteProjectParams {
    /// `delete` (default) removes the folder from disk; `keep` only removes the project
    /// from LiMa, leaves the files and adds the folder to the ignore list.
    pub files: Option<String>,
}

#[utoipa::path(delete,
    path = "/api/projects/{project_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project to delete"),
        DeleteProjectParams,
    ),
    responses(
        (status = 200, description = "Project deleted successfully"),
        (status = 400, description = "Unknown `files` mode", body = ApiErrorBody),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 500, description = "Failed to delete project", body = ApiErrorBody),
    )
)]
pub async fn project_delete(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(params): Query<DeleteProjectParams>,
) -> Result<StatusCode, ApiErrorResponse> {
    use lima_db::queries::projects_delete::{delete_project, remove_project_keep_files};

    let result = match params.files.as_deref() {
        None | Some("delete") => delete_project(state.db.pool(), &state.paths, &project_id).await,
        Some("keep") => remove_project_keep_files(state.db.pool(), &project_id, &crate::backups::now()).await,
        Some(_) => {
            return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_files_mode", "files must be delete or keep"));
        }
    };
    match result {
        Ok(_) => {
            let thumbs = state.paths.thumbnails();
            let project = project_id.clone();
            let removed = tokio::task::spawn_blocking(move || lima_indexer::thumbnails::remove_project(&thumbs, &project)).await;
            if let Ok(Err(e)) = removed {
                tracing::warn!("Failed to remove thumbnails of project {project_id}: {e}");
            }
            Ok(StatusCode::OK)
        }
        Err(DeleteProjectError::NotFound) => {
            return Err(ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            ));
        }
        Err(DeleteProjectError::DeleteFailed(msg)) => {
            return Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "project_delete_failed",
                "Project deletion failed",
            ).with_cause(&msg.to_string()));
        }
        Err(DeleteProjectError::Db(e)) => {
            return Err(ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                &"Database error occurred",
            ).with_cause(&e.to_string()));
        }
    }
}