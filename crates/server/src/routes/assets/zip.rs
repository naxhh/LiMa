//! Looking inside zip assets without extracting them: list the files, stream one out.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use lima_indexer::archive::{self, ArchiveError};
use serde::{Deserialize, Serialize};
use tokio_util::io::{ReaderStream, SyncIoBridge};
use utoipa::{IntoParams, ToSchema};

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct ZipEntry {
    /// Path inside the archive, `/`-separated.
    pub path: String,
    pub size: i64,
    pub compressed_size: i64,
    /// `model`, `image` or `other`, like assets.
    pub kind: String,
    /// Password protected: listed, but can't be opened.
    pub encrypted: bool,
    /// Whether it can be opened (not encrypted, supported compression).
    pub supported: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ZipContentsResponse {
    pub entries: Vec<ZipEntry>,
    /// Uncompressed size of all listed files.
    pub total_size: i64,
    /// The archive has more files than were listed (5,000).
    pub truncated: bool,
}

#[derive(Deserialize, IntoParams)]
pub struct ZipEntryParams {
    /// Path inside the archive, as listed.
    pub path: String,
    /// Send as a download instead of inline.
    #[serde(default)]
    pub download: bool,
}

/// Biggest single file streamed out of an archive.
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn not_found() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::NOT_FOUND, "asset_not_found", "Zip file not found")
}

fn archive_error(e: ArchiveError) -> ApiErrorResponse {
    match e {
        ArchiveError::NotFound => ApiErrorResponse::new(StatusCode::NOT_FOUND, "zip_entry_not_found", "No such file in the zip"),
        ArchiveError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => not_found(),
        ArchiveError::Encrypted => ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "zip_encrypted", "That file is password protected"),
        ArchiveError::Unsupported => ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "zip_unsupported", "That file uses an unsupported compression"),
        ArchiveError::TooLarge => ApiErrorResponse::new(StatusCode::PAYLOAD_TOO_LARGE, "zip_entry_too_large", "That file is too large"),
        ArchiveError::Invalid(m) => ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_zip", "Not a readable zip file").with_cause(&m),
        ArchiveError::Io(e) => ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "zip_read_failed", "Reading the zip failed").with_cause(&e.to_string()),
    }
}

/// The zip asset's path on disk.
async fn zip_path(state: &AppState, project_id: &str, asset_id: &str) -> Result<std::path::PathBuf, ApiErrorResponse> {
    let pool = state.db.pool();
    let db = |e: sqlx::Error| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string());
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT p.folder_path, a.file_path FROM assets a JOIN projects p ON p.id = a.project_id WHERE a.id = ?1 AND a.project_id = ?2",
    )
    .bind(asset_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    let (folder, file) = row.ok_or_else(not_found)?;
    if !archive::is_zip(&file) {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "not_a_zip", "That file isn't a zip"));
    }
    Ok(state.paths.project_dir(&folder).join(file))
}

#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/assets/{asset_id}/zip",
    tag = "Assets",
    params(("project_id" = String, Path), ("asset_id" = String, Path)),
    responses(
        (status = 200, description = "Files inside the zip (folders, links and OS clutter left out)", body = ZipContentsResponse),
        (status = 400, description = "Not a zip file", body = ApiErrorBody),
        (status = 404, description = "Asset not found", body = ApiErrorBody),
        (status = 422, description = "Not a readable zip", body = ApiErrorBody),
    )
)]
pub async fn zip_contents(State(state): State<AppState>, Path((project_id, asset_id)): Path<(String, String)>) -> Result<Json<ZipContentsResponse>, ApiErrorResponse> {
    let path = zip_path(&state, &project_id, &asset_id).await?;
    let listing = tokio::task::spawn_blocking(move || archive::list(&path))
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "zip_read_failed", "Reading the zip failed").with_cause(&e.to_string()))?
        .map_err(archive_error)?;
    let entries: Vec<ZipEntry> = listing
        .entries
        .into_iter()
        .map(|e| ZipEntry {
            kind: lima_domain::models::asset::asset_kind(&e.path).to_string(),
            size: e.size as i64,
            compressed_size: e.compressed_size as i64,
            encrypted: e.encrypted,
            supported: e.supported,
            path: e.path,
        })
        .collect();
    Ok(Json(ZipContentsResponse { total_size: entries.iter().map(|e| e.size).sum(), entries, truncated: listing.truncated }))
}

#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/assets/{asset_id}/zip/entry",
    tag = "Assets",
    params(("project_id" = String, Path), ("asset_id" = String, Path), ZipEntryParams),
    responses(
        (status = 200, description = "The file, streamed out of the zip"),
        (status = 404, description = "Asset or file not found", body = ApiErrorBody),
        (status = 422, description = "Password protected or unsupported compression", body = ApiErrorBody),
    )
)]
pub async fn zip_entry(
    State(state): State<AppState>,
    Path((project_id, asset_id)): Path<(String, String)>,
    Query(params): Query<ZipEntryParams>,
) -> Result<Response, ApiErrorResponse> {
    let path = zip_path(&state, &project_id, &asset_id).await?;

    // Check it first, so errors are proper responses rather than a cut-off stream.
    let (check_path, wanted) = (path.clone(), params.path.clone());
    let entry = tokio::task::spawn_blocking(move || archive::list(&check_path).map(|l| l.entries.into_iter().find(|e| e.path == wanted)))
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "zip_read_failed", "Reading the zip failed").with_cause(&e.to_string()))?
        .map_err(archive_error)?
        .ok_or_else(|| archive_error(ArchiveError::NotFound))?;
    if entry.encrypted {
        return Err(archive_error(ArchiveError::Encrypted));
    }
    if !entry.supported {
        return Err(archive_error(ArchiveError::Unsupported));
    }
    if entry.size > MAX_ENTRY_BYTES {
        return Err(archive_error(ArchiveError::TooLarge));
    }

    let (reader, writer) = tokio::io::duplex(256 * 1024);
    let name = entry.path.clone();
    tokio::task::spawn_blocking(move || {
        let mut sink = SyncIoBridge::new(writer);
        if let Err(e) = archive::read_entry(&path, &name, &mut sink, MAX_ENTRY_BYTES) {
            tracing::warn!("Streaming {name} out of {} failed: {e}", path.display());
        }
    });

    let file_name = entry.path.rsplit('/').next().unwrap_or(&entry.path).to_string();
    let mut res = Response::new(Body::from_stream(ReaderStream::new(reader)));
    let headers = res.headers_mut();
    let mime = crate::bundles::guess_mime(&file_name);
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(entry.size));
    if params.download {
        headers.insert(header::CONTENT_DISPOSITION, crate::routes::project_download::content_disposition(&file_name));
    }
    Ok(res)
}
