//! Collections: hand-picked groups of projects.

pub mod create;
pub mod delete;
pub mod detail;
pub mod list;
pub mod membership;
pub mod update;

use axum::http::StatusCode;
use lima_db::queries::collections::{CollectionRow, CoverImage};
use serde::Serialize;
use utoipa::ToSchema;

use crate::models::http_error::ApiErrorResponse;

/// Most cover images returned per collection.
pub(crate) const COVERS_PER_COLLECTION: i64 = 4;

#[derive(Serialize, ToSchema)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The built-in Favourites collection: always exists, can't be renamed or deleted.
    pub is_favourites: bool,
    pub project_count: i64,
    /// Main images of up to 4 of the most recently added projects, for previews.
    pub covers: Vec<CollectionCover>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionCover {
    pub project_id: String,
    pub asset_id: String,
}

impl Collection {
    pub(crate) fn from_row(row: CollectionRow, covers: Vec<CoverImage>) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            is_favourites: row.system_key.as_deref() == Some(lima_db::queries::collections::FAVOURITES_KEY),
            project_count: row.project_count,
            covers: covers
                .into_iter()
                .map(|c| CollectionCover { project_id: c.project_id, asset_id: c.asset_id })
                .collect(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub(crate) fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

pub(crate) fn db_error(e: impl ToString) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

pub(crate) fn not_found() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::NOT_FOUND, "collection_not_found", "Collection not found")
}

pub(crate) fn invalid_name() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_collection_name", "Collection name must be 1 to 100 characters")
}

pub(crate) fn invalid_description() -> ApiErrorResponse {
    ApiErrorResponse::new(
        StatusCode::BAD_REQUEST,
        "invalid_collection_description",
        "Collection description must be at most 2000 characters",
    )
}

pub(crate) fn protected() -> ApiErrorResponse {
    ApiErrorResponse::new(
        StatusCode::CONFLICT,
        "collection_protected",
        "Favourites is a built-in collection and can't be renamed or deleted",
    )
}

pub(crate) fn name_taken(existing_id: &str) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::CONFLICT, "collection_name_taken", "A collection with that name already exists")
        .with_details(serde_json::json!({ "collection_id": existing_id }))
}

/// Loads one collection with its covers, or 404.
pub(crate) async fn load(pool: &sqlx::Pool<sqlx::Sqlite>, id: &str) -> Result<Collection, ApiErrorResponse> {
    let row = lima_db::queries::collections::get_collection(pool, id).await.map_err(db_error)?.ok_or_else(not_found)?;
    let mut covers = lima_db::queries::collections::cover_images(pool, std::slice::from_ref(&row.id), COVERS_PER_COLLECTION)
        .await
        .map_err(db_error)?;
    let c = covers.remove(&row.id).unwrap_or_default();
    Ok(Collection::from_row(row, c))
}
