use axum::{ http::StatusCode, extract::{Path, State} };
use lima_db::queries::projects_delete::DeleteProjectError;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};


#[utoipa::path(delete,
    path = "/projects/{project_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project to delete"),
    ),
    responses(
        (status = 200, description = "Project deleted successfully"),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 500, description = "Failed to delete project", body = ApiErrorBody),
    )
)]
pub async fn project_delete(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {

    match lima_db::queries::projects_delete::delete_project(state.db.pool(), &project_id).await {
        Ok(_) => Ok(StatusCode::OK),
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