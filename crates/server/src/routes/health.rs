use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use crate::state::AppState;

pub async fn health_check(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    match lima_db::queries::ping(state.db.pool()).await {
        Ok(_) => Ok(Json(serde_json::json!({"db": true}))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}