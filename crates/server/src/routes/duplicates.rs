//! Duplicates: projects (or files) with the same content, and "keep one" to clean them up.

use std::collections::HashMap;

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::project::{ProjectListItem, with_summaries};
use crate::state::AppState;

/// Most projects one "keep one" may remove.
const MAX_REMOVE: usize = 500;

#[derive(Serialize, ToSchema)]
pub struct DuplicateProjectGroup {
    /// Identifies the group, for dismissing it.
    pub key: String,
    /// The projects, the suggested one to keep first (most tags/collections, then oldest).
    pub projects: Vec<DuplicateProject>,
    /// Model files every project in the group has.
    pub shared_models: i64,
    /// All projects have exactly the same model files (otherwise at least 80% the same).
    pub identical: bool,
    /// Space freed by keeping only the largest copy.
    pub reclaimable_bytes: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DuplicateProject {
    #[serde(flatten)]
    pub item: ProjectListItem,
    pub source_url: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SharedFileGroup {
    pub key: String,
    pub size_bytes: i64,
    pub kind: String,
    pub copies: Vec<SharedFileCopy>,
}

#[derive(Serialize, ToSchema)]
pub struct SharedFileCopy {
    pub project_id: String,
    pub project_name: String,
    pub asset_id: String,
    /// Relative to the project folder.
    pub file_path: String,
}

#[derive(Serialize, ToSchema)]
pub struct DuplicatesResponse {
    pub project_groups: Vec<DuplicateProjectGroup>,
    /// The same file in projects that aren't duplicates of each other. Shown for information.
    pub shared_files: Vec<SharedFileGroup>,
    /// Space freed by resolving every project group.
    pub reclaimable_bytes: i64,
    /// Files still waiting for a hash (sync is working on them); they can't be compared yet.
    pub unhashed_files: i64,
    /// Groups hidden as "not duplicates".
    pub dismissed: i64,
}

#[derive(Deserialize, ToSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemoveMode {
    /// Move the other copies to the trash (or delete them when the trash is off).
    Trash,
    /// Remove them from LiMa only: folders stay on disk and sync skips them.
    KeepFiles,
}

#[derive(Deserialize, ToSchema)]
pub struct ResolveDuplicatesRequest {
    pub keep_id: String,
    pub remove_ids: Vec<String>,
    pub remove: RemoveMode,
}

#[derive(Serialize, ToSchema)]
pub struct MergedMetadata {
    /// Tags added to the kept project.
    pub tags: u64,
    /// Collections (Favourites included) the kept project was added to.
    pub collections: u64,
    pub description: bool,
    pub source: bool,
    pub cover: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ResolveDuplicatesResponse {
    pub merged: MergedMetadata,
    /// Projects removed.
    pub removed: u64,
    /// Trash items created, to restore from.
    pub trash_ids: Vec<String>,
    /// Projects that couldn't be removed.
    pub failed: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct DismissDuplicatesRequest {
    /// The group's `key`.
    pub key: String,
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

fn bad(msg: &'static str) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_duplicates_request", msg)
}

/// How much the user has put into a project: what keeping it saves from being redone.
fn curation(p: &ProjectListItem) -> usize {
    p.tags.len() + p.collections.len() + usize::from(p.is_favourite) + usize::from(!p.project.description.trim().is_empty()) + usize::from(p.project.main_image_id.is_some())
}

#[utoipa::path(
    get,
    path = "/api/duplicates",
    tag = "Duplicates",
    responses(
        (status = 200, description = "Duplicate projects and shared files, biggest first", body = DuplicatesResponse),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn list_duplicates(State(state): State<AppState>) -> Result<Json<DuplicatesResponse>, ApiErrorResponse> {
    let pool = state.db.pool();
    let report = lima_db::queries::duplicates::find(pool).await.map_err(db_error)?;

    let mut ids: Vec<String> = report.project_groups.iter().flat_map(|g| g.project_ids.iter().cloned()).collect();
    ids.extend(report.shared_files.iter().flat_map(|f| f.entries.iter().map(|e| e.project_id.clone())));
    ids.sort();
    ids.dedup();
    let ids_json = serde_json::to_string(&ids).expect("serialize ids");
    let rows: Vec<(lima_domain::models::project::ProjectRow, Option<String>)> = {
        use sqlx::{FromRow, Row};
        sqlx::query(
            "SELECT id, folder_path, name, description, main_image_id, created_at, updated_at, last_scanned_at, source_url
             FROM projects WHERE id IN (SELECT value FROM json_each(?1))",
        )
        .bind(&ids_json)
        .fetch_all(pool)
        .await
        .map_err(db_error)?
        .iter()
        .map(|r| Ok((lima_domain::models::project::ProjectRow::from_row(r)?, r.try_get("source_url")?)))
        .collect::<Result<_, sqlx::Error>>()
        .map_err(db_error)?
    };
    let sources: HashMap<String, Option<String>> = rows.iter().map(|(p, s)| (p.id.clone(), s.clone())).collect();
    let names: HashMap<String, String> = rows.iter().map(|(p, _)| (p.id.clone(), p.name.clone())).collect();
    let mut items: HashMap<String, ProjectListItem> =
        with_summaries(&state, rows.into_iter().map(|(p, _)| p).collect()).await?.into_iter().map(|i| (i.project.id.clone(), i)).collect();

    let mut project_groups: Vec<DuplicateProjectGroup> = report
        .project_groups
        .into_iter()
        .map(|g| {
            let mut projects: Vec<DuplicateProject> = g
                .project_ids
                .iter()
                .filter_map(|id| items.remove(id))
                .map(|item| DuplicateProject { source_url: sources.get(&item.project.id).cloned().flatten(), item })
                .collect();
            projects.sort_by(|a, b| curation(&b.item).cmp(&curation(&a.item)).then_with(|| a.item.project.created_at.cmp(&b.item.project.created_at)));
            let total: i64 = projects.iter().map(|p| p.item.total_bytes).sum();
            let largest = projects.iter().map(|p| p.item.total_bytes).max().unwrap_or(0);
            DuplicateProjectGroup { key: g.key, projects, shared_models: g.shared_models, identical: g.identical, reclaimable_bytes: total - largest }
        })
        .filter(|g| g.projects.len() > 1)
        .collect();
    project_groups.sort_by_key(|g| std::cmp::Reverse(g.reclaimable_bytes));

    let shared_files = report
        .shared_files
        .into_iter()
        .map(|f| SharedFileGroup {
            key: f.key,
            size_bytes: f.entries.first().map(|e| e.size_bytes).unwrap_or(0),
            kind: f.entries.first().map(|e| e.kind.clone()).unwrap_or_default(),
            copies: f
                .entries
                .into_iter()
                .map(|e| SharedFileCopy { project_name: names.get(&e.project_id).cloned().unwrap_or_default(), project_id: e.project_id, asset_id: e.asset_id, file_path: e.file_path })
                .collect(),
        })
        .collect();

    Ok(Json(DuplicatesResponse {
        reclaimable_bytes: project_groups.iter().map(|g| g.reclaimable_bytes).sum(),
        project_groups,
        shared_files,
        unhashed_files: report.unhashed_files,
        dismissed: report.dismissed,
    }))
}

#[utoipa::path(
    post,
    path = "/api/duplicates/resolve",
    tag = "Duplicates",
    request_body = ResolveDuplicatesRequest,
    responses(
        (status = 200, description = "The others' tags, collections, favourite, and (where missing) description, source and cover were merged into the kept project, then the others were removed", body = ResolveDuplicatesResponse),
        (status = 400, description = "Nothing to remove, too many, or the kept project is also in the list", body = ApiErrorBody),
        (status = 404, description = "The project to keep doesn't exist", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn resolve_duplicates(State(state): State<AppState>, Json(req): Json<ResolveDuplicatesRequest>) -> Result<Json<ResolveDuplicatesResponse>, ApiErrorResponse> {
    let mut remove = req.remove_ids;
    remove.sort();
    remove.dedup();
    if remove.is_empty() {
        return Err(bad("remove_ids must not be empty"));
    }
    if remove.len() > MAX_REMOVE {
        return Err(bad("too many projects in one request"));
    }
    if remove.contains(&req.keep_id) {
        return Err(bad("the kept project can't also be removed"));
    }
    let merged = lima_db::queries::duplicates::merge_into(state.db.pool(), &req.keep_id, &remove, &crate::backups::now())
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "The project to keep doesn't exist"))?;
    let out = crate::routes::project_bulk::delete_many(&state, &remove, req.remove == RemoveMode::KeepFiles).await;
    Ok(Json(ResolveDuplicatesResponse {
        merged: MergedMetadata { tags: merged.tags, collections: merged.collections, description: merged.description, source: merged.source, cover: merged.cover },
        removed: out.affected,
        trash_ids: out.trash_ids,
        failed: out.failed,
    }))
}

#[utoipa::path(
    post,
    path = "/api/duplicates/dismiss",
    tag = "Duplicates",
    request_body = DismissDuplicatesRequest,
    responses(
        (status = 204, description = "Hidden as \"not duplicates\". It shows again if the group changes."),
        (status = 400, description = "Missing key", body = ApiErrorBody),
    )
)]
pub async fn dismiss_duplicates(State(state): State<AppState>, Json(req): Json<DismissDuplicatesRequest>) -> Result<StatusCode, ApiErrorResponse> {
    let key = req.key.trim();
    if key.is_empty() || key.len() > 100_000 {
        return Err(bad("key is required"));
    }
    lima_db::queries::duplicates::dismiss(state.db.pool(), key, &crate::backups::now()).await.map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/duplicates/dismissed",
    tag = "Duplicates",
    responses((status = 204, description = "Every dismissed group shows again"))
)]
pub async fn clear_dismissed(State(state): State<AppState>) -> Result<StatusCode, ApiErrorResponse> {
    lima_db::queries::duplicates::clear_dismissed(state.db.pool()).await.map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}
