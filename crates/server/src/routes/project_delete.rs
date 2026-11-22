use axum::{ Json, http::StatusCode, extract::{Path, Query, State} };
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use lima_db::queries::projects_delete::DeleteProjectError;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};
use crate::trash::TrashError;


#[derive(Deserialize, IntoParams)]
pub struct DeleteProjectParams {
    /// `delete` (default) moves the folder to the trash (or deletes it for good when the trash
    /// is disabled); `keep` only removes the project from LiMa, leaves the files and adds the
    /// folder to the ignore list.
    pub files: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DeleteProjectResponse {
    /// The trash item to restore it from; null when nothing went to the trash.
    pub trash_id: Option<String>,
}

#[utoipa::path(delete,
    path = "/api/projects/{project_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project to delete"),
        DeleteProjectParams,
    ),
    responses(
        (status = 200, description = "Project deleted (or moved to the trash)", body = DeleteProjectResponse),
        (status = 400, description = "Unknown `files` mode", body = ApiErrorBody),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 500, description = "Failed to delete project", body = ApiErrorBody),
    )
)]
pub async fn project_delete(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(params): Query<DeleteProjectParams>,
) -> Result<Json<DeleteProjectResponse>, ApiErrorResponse> {
    use lima_db::queries::projects_delete::{delete_project, remove_project_keep_files};

    let not_found = || ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "Project not found");
    let result = match params.files.as_deref() {
        None | Some("delete") if state.trash_days > 0 => {
            return match crate::trash::trash_project(&state, &project_id).await {
                Ok(id) => Ok(Json(DeleteProjectResponse { trash_id: Some(id) })),
                Err(TrashError::NotFound) => Err(not_found()),
                Err(TrashError::Io(msg)) => Err(ApiErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "project_delete_failed",
                    "Couldn't move the project to the trash",
                )
                .with_cause(&msg)),
                Err(TrashError::Db(e)) => Err(db_error(e)),
            };
        }
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
            Ok(Json(DeleteProjectResponse { trash_id: None }))
        }
        Err(DeleteProjectError::NotFound) => Err(not_found()),
        Err(DeleteProjectError::DeleteFailed(msg)) => Err(ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "project_delete_failed",
            "Project deletion failed",
        )
        .with_cause(&msg)),
        Err(DeleteProjectError::Db(e)) => Err(db_error(e)),
    }
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "database_error", "Database error occurred").with_cause(&e.to_string())
}
