//! A decimated mesh for the in-browser 3D viewer.
//!
//! Large slicer 3MFs are too big to parse in the browser without freezing. This endpoint
//! parses the model on the server (never on the request thread), reduces it to a triangle
//! budget with vertex clustering, and caches the result by file hash.

use std::path::Path as FsPath;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};

use crate::models::http_error::ApiErrorResponse;
use crate::state::AppState;

const MAX_TRIANGLES: usize = lima_indexer::preview_mesh::TARGET_TRIANGLES;

fn not_found() -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::NOT_FOUND, "asset_not_found", "Model not found in the specified project")
}

/// Cached file name for a content hash; `None` when the hash isn't a plain hex string.
fn cache_key(hash: Option<&str>) -> Option<String> {
    hash.filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).map(|h| format!("{h}.mesh"))
}

fn binary(bytes: Vec<u8>, immutable: bool) -> Response {
    let cache = if immutable { "public, max-age=31536000, immutable" } else { "no-cache" };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/assets/{asset_id}/preview",
    tag = "Assets",
    params(("project_id" = String, Path), ("asset_id" = String, Path)),
    responses(
        (status = 200, description = "Decimated binary mesh: `LMP1`, vertex count (u32 LE), index count (u32 LE), positions (f32x3), normals (f32x3), indices (u32)."),
        (status = 400, description = "Not a model file"),
        (status = 404, description = "Model not found in the specified project"),
        (status = 422, description = "The model could not be read"),
    )
)]
pub async fn asset_preview(
    State(state): State<AppState>,
    Path((project_id, asset_id)): Path<(String, String)>,
) -> Result<Response, ApiErrorResponse> {
    let pool = state.db.pool();
    let db = |e: sqlx::Error| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string());
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT p.folder_path, a.file_path, a.file_hash FROM assets a JOIN projects p ON p.id = a.project_id WHERE a.id = ?1 AND a.project_id = ?2",
    )
    .bind(&asset_id)
    .bind(&project_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    let (folder, file, hash) = row.ok_or_else(not_found)?;
    if !lima_indexer::model_thumbs::is_model(FsPath::new(&file)) {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "not_a_model", "That file isn't an STL, OBJ or 3MF model"));
    }

    let cached = cache_key(hash.as_deref()).map(|name| state.paths.preview_meshes().join(name));
    if let Some(path) = &cached
        && let Ok(bytes) = tokio::fs::read(path).await
    {
        return Ok(binary(bytes, true));
    }

    let source = state.paths.project_dir(&folder).join(&file);
    let cache_path = cached.clone();
    let built = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let tris = lima_indexer::model_thumbs::mesh(&source)?;
        let mesh = lima_indexer::preview_mesh::build(&tris, MAX_TRIANGLES);
        if mesh.indices.is_empty() {
            return Err("the model has no triangles".to_string());
        }
        let bytes = lima_indexer::preview_mesh::encode(&mesh);
        if let Some(path) = cache_path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        }
        tracing::info!("preview mesh built: {} triangles, {} bytes", mesh.triangle_count(), bytes.len());
        Ok(bytes)
    })
    .await
    .map_err(|e| ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "preview_failed", "Building the preview failed").with_cause(&e.to_string()))?
    .map_err(|e| ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "preview_failed", "The model couldn't be read").with_cause(&e))?;

    Ok(binary(built, cached.is_some()))
}
