use axum::{Json, extract::{Path, Query, State}};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{Collection, db_error, load};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::project::{ProjectListItem, with_summaries};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct CollectionDetailResponse {
    #[serde(flatten)]
    pub collection: Collection,
    /// Projects in the collection, in the requested order (default: most recently added first).
    pub projects: Vec<ProjectListItem>,
}

#[derive(Deserialize, IntoParams)]
pub struct CollectionDetailParams {
    /// added (default; when it was added to the collection), or any project sort:
    /// updated, created, modified, name, size, files.
    pub sort: Option<String>,
    /// asc or desc (default: asc for name, desc otherwise).
    pub order: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/collections/{collection_id}",
    tag = "Collections",
    params(("collection_id" = String, Path, description = "Collection id"), CollectionDetailParams),
    responses(
        (status = 200, description = "The collection and its projects", body = CollectionDetailResponse),
        (status = 400, description = "Unknown sort or order", body = ApiErrorBody),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn collection_detail(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(params): Query<CollectionDetailParams>,
) -> Result<Json<CollectionDetailResponse>, ApiErrorResponse> {
    let (sort, descending) = match params.sort.as_deref() {
        None | Some("added") => (None, crate::routes::project::parse_order(params.order.as_deref(), false)?),
        other => {
            let (s, d) = crate::routes::project::parse_sort(other, params.order.as_deref())?;
            (Some(s), d)
        }
    };
    let pool = state.db.pool();
    let collection = load(pool, &collection_id).await?;
    let projects = lima_db::queries::collections::collection_projects(pool, &collection_id, sort, descending)
        .await
        .map_err(db_error)?;
    let projects = with_summaries(&state, projects).await?;
    Ok(Json(CollectionDetailResponse { collection, projects }))
}
