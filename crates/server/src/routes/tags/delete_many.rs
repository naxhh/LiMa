use axum::{
    Json,
    extract::State,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

const MAX_BULK_TAGS: usize = 1000;

#[derive(Deserialize, ToSchema)]
pub struct DeleteTagsRequest {
    /// Ids of the tags to delete (max 1000).
    pub tag_ids: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DeleteTagsResponse {
    /// Number of tags actually deleted.
    pub deleted: i64,
}

#[utoipa::path(
    post,
    path = "/api/tags/delete",
    tag = "Tags",
    request_body = DeleteTagsRequest,
    responses(
        (status = 200, description = "Tags deleted", body = DeleteTagsResponse),
        (status = 400, description = "Too many tag ids", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn delete_tags(
    State(state): State<AppState>,
    Json(payload): Json<DeleteTagsRequest>,
) -> Result<Json<DeleteTagsResponse>, ApiErrorResponse> {
    if payload.tag_ids.len() > MAX_BULK_TAGS {
        return Err(ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "too_many_tags",
            "At most 1000 tags can be deleted at once",
        ));
    }

    let deleted = lima_db::queries::tags::delete_tags(state.db.pool(), &payload.tag_ids)
        .await
        .map_err(|e| {
            ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to delete tags").with_cause(&e.to_string())
        })?;

    Ok(Json(DeleteTagsResponse { deleted }))
}
