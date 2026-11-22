use axum::{Json, extract::State};
use lima_domain::models::collection::{normalize_collection_description, normalize_collection_name};
use serde::Deserialize;
use utoipa::ToSchema;

use super::{Collection, db_error, invalid_description, invalid_name, name_taken, now};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct CreateCollectionRequest {
    /// Trimmed, 1 to 100 characters. Unique ignoring case.
    pub name: String,
    /// Up to 2000 characters.
    pub description: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/collections",
    tag = "Collections",
    request_body = CreateCollectionRequest,
    responses(
        (status = 200, description = "Collection created", body = Collection),
        (status = 400, description = "Invalid name or description", body = ApiErrorBody),
        (status = 409, description = "A collection with that name already exists", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn create_collection(
    State(state): State<AppState>,
    Json(payload): Json<CreateCollectionRequest>,
) -> Result<Json<Collection>, ApiErrorResponse> {
    let name = normalize_collection_name(&payload.name).ok_or_else(invalid_name)?;
    let description =
        normalize_collection_description(payload.description.as_deref().unwrap_or("")).ok_or_else(invalid_description)?;
    let pool = state.db.pool();

    if let Some(existing) = lima_db::queries::collections::find_collection_id_by_name(pool, &name).await.map_err(db_error)? {
        return Err(name_taken(&existing));
    }
    let row = lima_db::queries::collections::create_collection(pool, &name, &description, &now())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => name_taken(""),
            _ => db_error(e),
        })?;
    Ok(Json(Collection::from_row(row, Vec::new())))
}
