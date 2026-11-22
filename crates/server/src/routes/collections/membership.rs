use axum::{Json, extract::{Path, State}, http::StatusCode};
use lima_db::queries::collections::MembershipError;
use serde::Deserialize;
use utoipa::ToSchema;

use super::{db_error, not_found, now};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct AddCollectionProjectRequest {
    pub project_id: String,
}

fn map_err(e: MembershipError) -> ApiErrorResponse {
    match e {
        MembershipError::CollectionNotFound => not_found(),
        MembershipError::ProjectNotFound => {
            ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "Project not found")
        }
        MembershipError::Db(e) => db_error(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/collections/{collection_id}/projects",
    tag = "Collections",
    params(("collection_id" = String, Path, description = "Collection id")),
    request_body = AddCollectionProjectRequest,
    responses(
        (status = 204, description = "Project is in the collection (adding it again is a no-op)"),
        (status = 404, description = "Collection or project not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn add_project(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Json(payload): Json<AddCollectionProjectRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    lima_db::queries::collections::add_project(state.db.pool(), &collection_id, &payload.project_id, &now())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/collections/{collection_id}/projects/{project_id}",
    tag = "Collections",
    params(
        ("collection_id" = String, Path, description = "Collection id"),
        ("project_id" = String, Path, description = "Project id"),
    ),
    responses(
        (status = 204, description = "Project is not in the collection any more"),
        (status = 404, description = "Collection or project not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn remove_project(
    State(state): State<AppState>,
    Path((collection_id, project_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiErrorResponse> {
    lima_db::queries::collections::remove_project(state.db.pool(), &collection_id, &project_id, &now())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
