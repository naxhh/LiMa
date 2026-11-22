use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use super::update::not_found;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[utoipa::path(
    delete,
    path = "/api/tags/{tag_id}",
    tag = "Tags",
    params(("tag_id" = String, Path, description = "Tag id")),
    responses(
        (status = 204, description = "Tag deleted and removed from all projects"),
        (status = 404, description = "Tag not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn delete_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {
    match lima_db::queries::tags::delete_tag(state.db.pool(), &tag_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(not_found()),
        Err(e) => Err(ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to delete tag")
            .with_cause(&e.to_string())),
    }
}
