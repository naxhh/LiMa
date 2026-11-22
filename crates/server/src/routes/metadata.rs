//! Metadata backup and restore.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use lima_db::queries::metadata::RestoreError;
use lima_domain::models::metadata::{MetadataBackup, RestoreReport};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::backups::{self, BackupFile};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

fn db_error(e: impl ToString) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

fn backup_error(e: impl ToString) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "backup_failed", "Could not read or write backups")
        .with_cause(&e.to_string())
}

fn not_found() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::NOT_FOUND, "backup_not_found", "Backup not found")
}

fn json_download(name: &str, bytes: Vec<u8>) -> Response {
    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

#[utoipa::path(
    get,
    path = "/api/metadata/export",
    tag = "Metadata",
    responses(
        (status = 200, description = "Every tag, collection and project's metadata as a JSON file", body = MetadataBackup),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn export_metadata(State(state): State<AppState>) -> Result<Response, ApiErrorResponse> {
    let backup = lima_db::queries::metadata::export_metadata(state.db.pool(), &backups::now()).await.map_err(db_error)?;
    let bytes = serde_json::to_vec_pretty(&backup).map_err(backup_error)?;
    let day = backup.exported_at.get(..10).unwrap_or("export");
    Ok(json_download(&format!("lima-metadata-{day}.json"), bytes))
}

#[derive(Deserialize, IntoParams)]
pub struct RestoreParams {
    /// Report what would change without writing anything.
    #[serde(default)]
    pub dry_run: bool,
}

async fn restore(state: &AppState, backup: &MetadataBackup, dry_run: bool) -> Result<Json<RestoreReport>, ApiErrorResponse> {
    let report = lima_db::queries::metadata::restore_metadata(state.db.pool(), backup, dry_run, &backups::now())
        .await
        .map_err(|e| match e {
            RestoreError::UnsupportedFormat(msg) => {
                ApiErrorResponse::new(StatusCode::BAD_REQUEST, "unsupported_backup", "Not a supported LiMa metadata backup")
                    .with_cause(&msg)
            }
            RestoreError::Db(e) => db_error(e),
        })?;
    if !dry_run {
        tracing::info!(
            "Metadata restored: {} by folder, {} renamed, {} unmatched",
            report.matched_by_folder,
            report.matched_by_files.len(),
            report.unmatched.len()
        );
    }
    Ok(Json(report))
}

#[utoipa::path(
    post,
    path = "/api/metadata/restore",
    tag = "Metadata",
    params(RestoreParams),
    request_body = MetadataBackup,
    responses(
        (status = 200, description = "What was restored (or would be, for a dry run)", body = RestoreReport),
        (status = 400, description = "Not a LiMa metadata backup, or from a newer version", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn restore_upload(
    State(state): State<AppState>,
    Query(params): Query<RestoreParams>,
    Json(backup): Json<MetadataBackup>,
) -> Result<Json<RestoreReport>, ApiErrorResponse> {
    restore(&state, &backup, params.dry_run).await
}

#[derive(Serialize, ToSchema)]
pub struct ListBackupsResponse {
    pub items: Vec<BackupFile>,
    /// Where backups are written (`LIMA_BACKUP_DIR`).
    pub directory: String,
    /// Hours between automatic backups; `null` when disabled.
    pub interval_hours: Option<u64>,
    /// How many backups are kept.
    pub keep: usize,
}

#[utoipa::path(
    get,
    path = "/api/metadata/backups",
    tag = "Metadata",
    responses(
        (status = 200, description = "Stored backups, newest first", body = ListBackupsResponse),
        (status = 500, description = "Backup folder unreadable", body = ApiErrorBody),
    )
)]
pub async fn list_backups(State(state): State<AppState>) -> Result<Json<ListBackupsResponse>, ApiErrorResponse> {
    let dir = state.paths.backups.clone();
    let items = tokio::task::spawn_blocking(move || backups::list(&dir)).await.map_err(backup_error)?.map_err(backup_error)?;
    Ok(Json(ListBackupsResponse {
        items,
        directory: state.paths.backups.display().to_string(),
        interval_hours: state.auto_backup.map(|a| a.interval.as_secs() / 3600),
        keep: state.backup_keep,
    }))
}

#[utoipa::path(
    post,
    path = "/api/metadata/backups",
    tag = "Metadata",
    responses(
        (status = 200, description = "Backup written", body = BackupFile),
        (status = 500, description = "Could not write the backup", body = ApiErrorBody),
    )
)]
pub async fn create_backup(State(state): State<AppState>) -> Result<Json<BackupFile>, ApiErrorResponse> {
    let file = backups::write_backup(state.db.pool(), &state.paths.backups, state.backup_keep, false)
        .await
        .map_err(backup_error)?
        .ok_or_else(|| backup_error("backup was not written"))?;
    Ok(Json(file))
}

#[utoipa::path(
    get,
    path = "/api/metadata/backups/{name}",
    tag = "Metadata",
    params(("name" = String, Path, description = "Backup file name")),
    responses(
        (status = 200, description = "The backup file", body = MetadataBackup),
        (status = 404, description = "Backup not found", body = ApiErrorBody),
    )
)]
pub async fn download_backup(State(state): State<AppState>, Path(name): Path<String>) -> Result<Response, ApiErrorResponse> {
    let path = state.paths.backup_file(&name).ok_or_else(not_found)?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => not_found(),
        _ => backup_error(e),
    })?;
    Ok(json_download(&name, bytes))
}

#[utoipa::path(
    post,
    path = "/api/metadata/backups/{name}/restore",
    tag = "Metadata",
    params(("name" = String, Path, description = "Backup file name"), RestoreParams),
    responses(
        (status = 200, description = "What was restored (or would be, for a dry run)", body = RestoreReport),
        (status = 400, description = "Not a LiMa metadata backup, or from a newer version", body = ApiErrorBody),
        (status = 404, description = "Backup not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn restore_backup(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<RestoreParams>,
) -> Result<Json<RestoreReport>, ApiErrorResponse> {
    let path = state.paths.backup_file(&name).ok_or_else(not_found)?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => not_found(),
        _ => backup_error(e),
    })?;
    let backup: MetadataBackup = serde_json::from_slice(&bytes).map_err(|e| {
        ApiErrorResponse::new(StatusCode::BAD_REQUEST, "unsupported_backup", "Not a supported LiMa metadata backup")
            .with_cause(&e.to_string())
    })?;
    restore(&state, &backup, params.dry_run).await
}
