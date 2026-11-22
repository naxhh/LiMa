use axum::{Json, extract::State, http::StatusCode};
use lima_domain::models::tag::{normalize_tag_color, normalize_tag_name};
use serde::Deserialize;
use utoipa::ToSchema;

use super::list::Tag;
use super::now;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct CreateTagRequest {
    /// Trimmed, 1 to 64 characters. Unique ignoring case.
    pub name: String,
    /// `#RRGGBB`. Derived from the name when omitted.
    pub color: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/tags",
    tag = "Tags",
    request_body = CreateTagRequest,
    responses(
        (status = 200, description = "Tag created", body = Tag),
        (status = 400, description = "Invalid name or color", body = ApiErrorBody),
        (status = 409, description = "A tag with that name already exists", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn create_tag(
    State(state): State<AppState>,
    Json(payload): Json<CreateTagRequest>,
) -> Result<Json<Tag>, ApiErrorResponse> {
    let name = normalize_tag_name(&payload.name).ok_or_else(invalid_name)?;
    let color = payload.color.as_deref().map(|c| normalize_tag_color(c).ok_or_else(invalid_color)).transpose()?;
    let pool = state.db.pool();

    let db_err = |e: sqlx::Error| {
        ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to create tag").with_cause(&e.to_string())
    };
    if let Some(existing) = lima_db::queries::tags::find_tag_id_by_name(pool, &name).await.map_err(db_err)? {
        return Err(name_taken(&existing));
    }

    let tag = lima_db::queries::tags::create_tag(pool, &name, color.as_deref(), &now())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => name_taken(""),
            _ => db_err(e),
        })?;

    Ok(Json(tag.into()))
}

pub(super) fn invalid_name() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_tag_name", "Tag name must be 1 to 64 characters")
}

pub(super) fn invalid_color() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_tag_color", "Color must be a #RRGGBB hex value")
}

pub(super) fn name_taken(existing_id: &str) -> ApiErrorResponse {
    let err = ApiErrorResponse::new(StatusCode::CONFLICT, "tag_name_taken", "A tag with that name already exists");
    if existing_id.is_empty() { err } else { err.with_details(serde_json::json!({ "tag_id": existing_id })) }
}
