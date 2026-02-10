use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use utoipa::ToSchema;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    db: bool,
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Health check successful", body = HealthResponse),
        (status = 503, description = "Database connection failed")
    )
)]
pub async fn health_check(
    State(state): State<AppState>
) -> Result<Json<HealthResponse>, StatusCode> {
    match lima_db::queries::ping(state.db.pool()).await {
        Ok(_) => Ok(Json(HealthResponse { db: true })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}