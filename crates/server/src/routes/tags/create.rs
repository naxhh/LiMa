use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use utoipa::ToSchema;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};

#[derive(Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateTagResponse {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

#[utoipa::path(
    post,
    path = "/tags",
    request_body = CreateTagRequest,
    responses(
        (status = 200, description = "Tag created successfully", body = CreateTagResponse),
        (status = 400, description = "Invalid request body", body = ApiErrorBody),
        (status = 503, description = "Failure to connect to the database", body = ApiErrorBody),
    )
)]
pub async fn create_tag(
    State(state): State<AppState>,
    Json(payload): Json<CreateTagRequest>,
) -> Result<Json<CreateTagResponse>, ApiErrorResponse> {
    let now: String = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "time_format_error",
            "Failed to format current time: {}",
        ).with_cause(&e.to_string())
    })?;

    let tag = lima_db::queries::tags::create_tag(
        state.db.pool(),
        &payload.name,
        &now,
    ).await.map_err(|e| ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "Failed to create tag").with_cause(&e.to_string()))?;

    Ok(Json(CreateTagResponse {
        id: tag.id.to_string(),
        name: tag.name,
        color: tag.color,
        created_at: tag.created_at,
        updated_at: tag.updated_at,
    }))
}
