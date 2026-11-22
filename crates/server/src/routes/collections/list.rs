use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use super::{COVERS_PER_COLLECTION, Collection, db_error};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct ListCollectionsResponse {
    pub items: Vec<Collection>,
}

#[utoipa::path(
    get,
    path = "/api/collections",
    tag = "Collections",
    responses(
        (status = 200, description = "Every collection, sorted by name", body = ListCollectionsResponse),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_collections(State(state): State<AppState>) -> Result<Json<ListCollectionsResponse>, ApiErrorResponse> {
    let pool = state.db.pool();
    let rows = lima_db::queries::collections::list_collections(pool).await.map_err(db_error)?;
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let mut covers = lima_db::queries::collections::cover_images(pool, &ids, COVERS_PER_COLLECTION)
        .await
        .map_err(db_error)?;
    let items = rows
        .into_iter()
        .map(|r| {
            let c = covers.remove(&r.id).unwrap_or_default();
            Collection::from_row(r, c)
        })
        .collect();
    Ok(Json(ListCollectionsResponse { items }))
}
