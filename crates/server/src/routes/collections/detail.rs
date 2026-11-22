use axum::{Json, extract::{Path, State}};
use serde::Serialize;
use utoipa::ToSchema;

use super::{Collection, db_error, load};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::project::{ProjectListItem, with_summaries};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct CollectionDetailResponse {
    #[serde(flatten)]
    pub collection: Collection,
    /// Projects in the collection, most recently added first.
    pub projects: Vec<ProjectListItem>,
}

#[utoipa::path(
    get,
    path = "/api/collections/{collection_id}",
    tag = "Collections",
    params(("collection_id" = String, Path, description = "Collection id")),
    responses(
        (status = 200, description = "The collection and its projects", body = CollectionDetailResponse),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn collection_detail(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
) -> Result<Json<CollectionDetailResponse>, ApiErrorResponse> {
    let pool = state.db.pool();
    let collection = load(pool, &collection_id).await?;
    let projects = lima_db::queries::collections::collection_projects(pool, &collection_id).await.map_err(db_error)?;
    let projects = with_summaries(&state, projects).await?;
    Ok(Json(CollectionDetailResponse { collection, projects }))
}
