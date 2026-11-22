use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use lima_db::queries::tags::UpdateTagError;
use lima_domain::models::tag::{normalize_tag_color, normalize_tag_name};
use serde::Deserialize;
use utoipa::ToSchema;

use super::create::{invalid_color, invalid_name, name_taken};
use super::list::Tag;
use super::now;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct PatchTagRequest {
    /// New name. Trimmed, 1 to 64 characters, unique ignoring case.
    pub name: Option<String>,
    /// New `#RRGGBB` color.
    pub color: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/tags/{tag_id}",
    tag = "Tags",
    request_body = PatchTagRequest,
    params(("tag_id" = String, Path, description = "Tag id")),
    responses(
        (status = 200, description = "Updated tag", body = Tag),
        (status = 400, description = "Nothing to update, or invalid name/color", body = ApiErrorBody),
        (status = 404, description = "Tag not found", body = ApiErrorBody),
        (status = 409, description = "Another tag already uses that name", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn update_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<String>,
    Json(payload): Json<PatchTagRequest>,
) -> Result<Json<Tag>, ApiErrorResponse> {
    if payload.name.is_none() && payload.color.is_none() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "missing_fields", "Provide a name and/or color"));
    }
    let name = payload.name.as_deref().map(|n| normalize_tag_name(n).ok_or_else(invalid_name)).transpose()?;
    let color = payload.color.as_deref().map(|c| normalize_tag_color(c).ok_or_else(invalid_color)).transpose()?;

    lima_db::queries::tags::update_tag(state.db.pool(), &tag_id, name.as_deref(), color.as_deref(), &now())
        .await
        .map(|row| Json(row.into()))
        .map_err(|e| match e {
            UpdateTagError::NotFound => not_found(),
            UpdateTagError::NameTaken => name_taken(""),
            UpdateTagError::Db(sqlx::Error::Database(d)) if d.is_unique_violation() => name_taken(""),
            UpdateTagError::Db(e) => {
                ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to update tag").with_cause(&e.to_string())
            }
        })
}

pub(super) fn not_found() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::NOT_FOUND, "tag_not_found", "Tag not found")
}
