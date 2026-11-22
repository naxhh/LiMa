use axum::{Json, extract::{Path, State}};
use lima_domain::models::collection::{normalize_collection_description, normalize_collection_name};
use serde::Deserialize;
use utoipa::ToSchema;

use super::{Collection, db_error, invalid_description, invalid_name, load, name_taken, not_found, now};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct PatchCollectionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/collections/{collection_id}",
    tag = "Collections",
    params(("collection_id" = String, Path, description = "Collection id")),
    request_body = PatchCollectionRequest,
    responses(
        (status = 200, description = "Collection updated", body = Collection),
        (status = 400, description = "Invalid name or description", body = ApiErrorBody),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 409, description = "A collection with that name already exists", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn update_collection(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Json(payload): Json<PatchCollectionRequest>,
) -> Result<Json<Collection>, ApiErrorResponse> {
    let name = payload.name.as_deref().map(|n| normalize_collection_name(n).ok_or_else(invalid_name)).transpose()?;
    let description = payload
        .description
        .as_deref()
        .map(|d| normalize_collection_description(d).ok_or_else(invalid_description))
        .transpose()?;
    let pool = state.db.pool();

    if let Some(ref name) = name {
        if let Some(existing) = lima_db::queries::collections::find_collection_id_by_name(pool, name).await.map_err(db_error)? {
            if existing != collection_id {
                return Err(name_taken(&existing));
            }
        }
    }
    let found = lima_db::queries::collections::update_collection(pool, &collection_id, name.as_deref(), description.as_deref(), &now())
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => name_taken(""),
            _ => db_error(e),
        })?;
    if !found {
        return Err(not_found());
    }
    Ok(Json(load(pool, &collection_id).await?))
}
