//! `POST /api/projects/bulk`: one action applied to many projects.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

/// Most projects one request may touch.
const MAX_PROJECTS: usize = 2000;

#[derive(Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BulkAction {
    /// Add tags (created if needed).
    AddTags { tags: Vec<String> },
    /// Remove tags by name; the tags themselves stay.
    RemoveTags { tags: Vec<String> },
    AddToCollection { collection_id: String },
    RemoveFromCollection { collection_id: String },
    Favourite,
    Unfavourite,
    /// Delete: to the trash when it's enabled, otherwise for good.
    Delete,
    /// Remove from LiMa only: folders stay on disk and are ignored by sync.
    RemoveKeepFiles,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkRequest {
    pub project_ids: Vec<String>,
    #[serde(flatten)]
    pub action: BulkAction,
}

#[derive(Serialize, ToSchema, Default)]
pub struct BulkResponse {
    /// Links added or removed, or projects deleted.
    pub affected: u64,
    /// Trash items created by `delete`, to restore from.
    pub trash_ids: Vec<String>,
    /// Projects that couldn't be changed (missing, or a move failed).
    pub failed: Vec<String>,
}

fn bad(msg: &'static str) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_bulk_request", msg)
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

fn clean_tags(tags: Vec<String>) -> Result<Vec<String>, ApiErrorResponse> {
    let tags: Vec<String> = tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    if tags.is_empty() {
        return Err(bad("tags must not be empty"));
    }
    if tags.iter().any(|t| t.chars().count() > 64) {
        return Err(bad("tag names are at most 64 characters"));
    }
    Ok(tags)
}

#[utoipa::path(
    post,
    path = "/api/projects/bulk",
    tag = "Projects",
    request_body = BulkRequest,
    responses(
        (status = 200, description = "Applied; missing projects are skipped", body = BulkResponse),
        (status = 400, description = "Empty or too many ids, or bad tags", body = ApiErrorBody),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn bulk(State(state): State<AppState>, Json(req): Json<BulkRequest>) -> Result<Json<BulkResponse>, ApiErrorResponse> {
    use lima_db::queries::bulk;

    let mut ids = req.project_ids;
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err(bad("project_ids must not be empty"));
    }
    if ids.len() > MAX_PROJECTS {
        return Err(bad("too many projects in one request"));
    }
    let pool = state.db.pool();
    let now = crate::backups::now();
    let collection_missing = || ApiErrorResponse::new(StatusCode::NOT_FOUND, "collection_not_found", "Collection not found");
    let favourites = || async { lima_db::queries::collections::ensure_favourites(pool, &now).await.map_err(db_error) };

    let affected = match req.action {
        BulkAction::AddTags { tags } => bulk::add_tags(pool, &ids, &clean_tags(tags)?, &now).await.map_err(db_error)?,
        BulkAction::RemoveTags { tags } => bulk::remove_tags(pool, &ids, &clean_tags(tags)?).await.map_err(db_error)?,
        BulkAction::AddToCollection { collection_id } => {
            bulk::add_to_collection(pool, &collection_id, &ids, &now).await.map_err(db_error)?.ok_or_else(collection_missing)?
        }
        BulkAction::RemoveFromCollection { collection_id } => {
            bulk::remove_from_collection(pool, &collection_id, &ids, &now).await.map_err(db_error)?.ok_or_else(collection_missing)?
        }
        BulkAction::Favourite => bulk::add_to_collection(pool, &favourites().await?, &ids, &now).await.map_err(db_error)?.unwrap_or(0),
        BulkAction::Unfavourite => bulk::remove_from_collection(pool, &favourites().await?, &ids, &now).await.map_err(db_error)?.unwrap_or(0),
        BulkAction::Delete => return Ok(Json(delete_many(&state, &ids, false).await)),
        BulkAction::RemoveKeepFiles => return Ok(Json(delete_many(&state, &ids, true).await)),
    };
    Ok(Json(BulkResponse { affected, ..Default::default() }))
}

/// Deletes (or removes from LiMa) one project at a time: each is a folder move plus a transaction.
pub(crate) async fn delete_many(state: &AppState, ids: &[String], keep_files: bool) -> BulkResponse {
    use lima_db::queries::projects_delete::{delete_project, remove_project_keep_files};
    let mut out = BulkResponse::default();
    for id in ids {
        let ok = if keep_files {
            remove_project_keep_files(state.db.pool(), id, &crate::backups::now()).await.is_ok()
        } else if state.trash_days > 0 {
            match crate::trash::trash_project(state, id).await {
                Ok(trash_id) => {
                    out.trash_ids.push(trash_id);
                    true
                }
                Err(e) => {
                    tracing::warn!("Bulk delete of {id} failed: {e:?}");
                    false
                }
            }
        } else {
            delete_project(state.db.pool(), &state.paths, id).await.is_ok()
        };
        if ok {
            out.affected += 1;
            if keep_files || state.trash_days == 0 {
                let thumbs = state.paths.thumbnails();
                let project = id.clone();
                let _ = tokio::task::spawn_blocking(move || lima_indexer::thumbnails::remove_project(&thumbs, &project)).await;
            }
        } else {
            out.failed.push(id.clone());
        }
    }
    out
}
