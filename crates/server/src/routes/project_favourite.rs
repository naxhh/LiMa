//! Star / unstar a project: membership of the built-in Favourites collection.

use axum::{extract::{Path, State}, http::StatusCode};
use lima_db::queries::collections::{MembershipError, add_project, ensure_favourites, remove_project};

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::collections::{db_error, now};
use crate::state::AppState;

fn map_err(e: MembershipError) -> ApiErrorResponse {
    match e {
        MembershipError::ProjectNotFound | MembershipError::CollectionNotFound => {
            ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "Project not found")
        }
        MembershipError::Db(e) => db_error(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/projects/{project_id}/favourite",
    tag = "Projects",
    params(("project_id" = String, Path, description = "Project id")),
    responses(
        (status = 204, description = "Project is in Favourites"),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn add_favourite(State(state): State<AppState>, Path(project_id): Path<String>) -> Result<StatusCode, ApiErrorResponse> {
    let pool = state.db.pool();
    let fav = ensure_favourites(pool, &now()).await.map_err(db_error)?;
    add_project(pool, &fav, &project_id, &now()).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/projects/{project_id}/favourite",
    tag = "Projects",
    params(("project_id" = String, Path, description = "Project id")),
    responses(
        (status = 204, description = "Project is not in Favourites"),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn remove_favourite(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {
    let pool = state.db.pool();
    let fav = ensure_favourites(pool, &now()).await.map_err(db_error)?;
    remove_project(pool, &fav, &project_id, &now()).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
