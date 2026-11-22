use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct LibraryStatsResponse {
    pub projects: i64,
    pub assets: i64,
    pub total_bytes: i64,
}

#[utoipa::path(
    get,
    path = "/api/stats",
    responses(
        (status = 200, description = "Library totals", body = LibraryStatsResponse),
        (status = 503, description = "Failure to connect to the database", body = ApiErrorBody),
    )
)]
pub async fn library_stats(State(state): State<AppState>) -> Result<Json<LibraryStatsResponse>, ApiErrorResponse> {
    let row = lima_db::queries::projects_summary::library_stats(state.db.pool())
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed reading stats").with_cause(&e.to_string()))?;
    Ok(Json(LibraryStatsResponse { projects: row.projects, assets: row.assets, total_bytes: row.total_bytes }))
}
