use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use lima_db::queries::tags::MergeTagError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::update::not_found;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct MergeTagRequest {
    /// Id of the tag to merge this tag into.
    pub into_tag_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct MergeTagResponse {
    /// Number of projects that gained the target tag.
    pub projects_affected: i64,
}

#[utoipa::path(
    post,
    path = "/api/tags/{tag_id}/merge",
    tag = "Tags",
    request_body = MergeTagRequest,
    params(("tag_id" = String, Path, description = "Tag to merge away")),
    responses(
        (status = 200, description = "Tag merged into another", body = MergeTagResponse),
        (status = 400, description = "Both tag ids are the same", body = ApiErrorBody),
        (status = 404, description = "Tag not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn merge_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<String>,
    Json(payload): Json<MergeTagRequest>,
) -> Result<Json<MergeTagResponse>, ApiErrorResponse> {
    lima_db::queries::tags::merge_tag(state.db.pool(), &tag_id, &payload.into_tag_id)
        .await
        .map(|projects_affected| Json(MergeTagResponse { projects_affected }))
        .map_err(|e| match e {
            MergeTagError::NotFound => not_found(),
            MergeTagError::SameTag => {
                ApiErrorResponse::new(StatusCode::BAD_REQUEST, "same_tag", "Can't merge a tag into itself")
            }
            MergeTagError::Db(e) => {
                ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to merge tags").with_cause(&e.to_string())
            }
        })
}
