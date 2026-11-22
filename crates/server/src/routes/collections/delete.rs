use axum::{extract::{Path, State}, http::StatusCode};

use super::{db_error, not_found};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[utoipa::path(
    delete,
    path = "/api/collections/{collection_id}",
    tag = "Collections",
    params(("collection_id" = String, Path, description = "Collection id")),
    responses(
        (status = 204, description = "Collection deleted. Its projects are not affected."),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn delete_collection(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
) -> Result<StatusCode, ApiErrorResponse> {
    let deleted = lima_db::queries::collections::delete_collection(state.db.pool(), &collection_id)
        .await
        .map_err(db_error)?;
    if deleted { Ok(StatusCode::NO_CONTENT) } else { Err(not_found()) }
}
