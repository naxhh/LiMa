//! Resumable uploads: the tus 1.0.0 core protocol plus the `creation` and `termination`
//! extensions, so a large bundle survives a reload or a flaky connection.
//!
//! Flow: `POST /api/bundles/new` → `POST /api/uploads` (one per file) → `PATCH` chunks →
//! the finished file lands in the bundle and waits for `POST /api/projects/{id}/import`.

use std::collections::HashMap;
use std::path::Path;

use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use futures_util::StreamExt;
use tokio::fs;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;
use crate::uploads::{UploadFiles, UploadInfo};

const TUS_VERSION: &str = "1.0.0";
const OFFSET_CONTENT_TYPE: &str = "application/offset+octet-stream";

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// A bodyless response, with the tus version every response carries.
fn empty_response(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response.headers_mut().insert("tus-resumable", HeaderValue::from_static(TUS_VERSION));
    response
}

/// An error in the API's JSON shape, still a tus response.
fn error_response(error: ApiErrorResponse) -> Response {
    let mut response = error.into_response();
    response.headers_mut().insert("tus-resumable", HeaderValue::from_static(TUS_VERSION));
    response
}

fn upload_not_found() -> Response {
    error_response(ApiErrorResponse::new(StatusCode::NOT_FOUND, "upload_not_found", "The upload does not exist or has finished."))
}

fn missing_tus_version() -> Response {
    error_response(ApiErrorResponse::new(StatusCode::PRECONDITION_FAILED, "tus_version_required", "Tus-Resumable: 1.0.0 is required."))
}

/// `key base64value` pairs, comma separated, as tus defines `Upload-Metadata`.
fn parse_metadata(raw: &str) -> Result<HashMap<String, String>, ()> {
    let mut fields = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (key, encoded) = match entry.split_once(' ') {
            Some((key, encoded)) => (key.trim(), encoded.trim()),
            None => (entry, ""),
        };
        if key.is_empty() {
            return Err(());
        }
        let value = if encoded.is_empty() {
            String::new()
        } else {
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| ())?;
            String::from_utf8(bytes).map_err(|_| ())?
        };
        fields.insert(key.to_ascii_lowercase(), value);
    }
    Ok(fields)
}

#[utoipa::path(
    options,
    path = "/api/uploads",
    description = "tus server discovery: supported version, extensions and the largest upload accepted.",
    responses(
        (status = 204, description = "Capabilities in the Tus-* headers"),
    )
)]
pub async fn upload_capabilities(State(state): State<AppState>) -> Response {
    let mut response = empty_response(StatusCode::NO_CONTENT);
    let headers = response.headers_mut();
    headers.insert("tus-version", HeaderValue::from_static(TUS_VERSION));
    headers.insert("tus-extension", HeaderValue::from_static("creation,termination"));
    headers.insert("tus-max-size", HeaderValue::from(state.upload_max_bytes));
    response
}

#[utoipa::path(
    post,
    path = "/api/uploads",
    description = "Creates an upload for one file of a bundle (tus creation extension).",
    params(
        ("Tus-Resumable" = String, Header, description = "Must be 1.0.0"),
        ("Upload-Length" = i64, Header, description = "Total size of the file, in bytes"),
        ("Upload-Metadata" = String, Header, description = "Comma-separated `key base64value` pairs: bundle_id (from POST /api/bundles/new), filename, optional mtime (ms)"),
    ),
    responses(
        (status = 201, description = "Upload created; its URL is in Location"),
        (status = 400, description = "Missing or invalid Upload-Length, Upload-Metadata, filename or mtime", body = ApiErrorBody),
        (status = 404, description = "The bundle does not exist", body = ApiErrorBody),
        (status = 412, description = "Tus-Resumable is missing or not 1.0.0", body = ApiErrorBody),
        (status = 413, description = "The upload is larger than Tus-Max-Size", body = ApiErrorBody),
        (status = 500, description = "Failed creating the upload files", body = ApiErrorBody),
    )
)]
pub async fn create_upload(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if header_str(&headers, "tus-resumable") != Some(TUS_VERSION) {
        return missing_tus_version();
    }
    let length = header_str(&headers, "upload-length")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|length| *length >= 0);
    let Some(length) = length else {
        return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "upload_length_required", "Upload-Length is required."));
    };
    if length as u64 > state.upload_max_bytes {
        return error_response(
            ApiErrorResponse::new(StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large", "The upload is larger than this server accepts.")
                .with_details(serde_json::json!({ "max_bytes": state.upload_max_bytes })),
        );
    }
    let metadata = match header_str(&headers, "upload-metadata").map(parse_metadata) {
        Some(Ok(metadata)) => metadata,
        Some(Err(())) => {
            return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_upload_metadata", "Upload-Metadata is not a list of `key base64value` pairs."));
        }
        None => {
            return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "upload_metadata_required", "Upload-Metadata is required."));
        }
    };
    let Some(bundle_id) = metadata.get("bundle_id").filter(|value| !value.is_empty()).cloned() else {
        return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "bundle_id_required", "Upload-Metadata needs a bundle_id."));
    };
    let Some(filename) = metadata.get("filename").and_then(|value| crate::bundles::sanitize_filename(value)) else {
        return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_filename", "Upload-Metadata needs a plain file name."));
    };
    let mtime = match metadata.get("mtime") {
        Some(text) => match text.trim().parse::<u64>() {
            Ok(ms) => Some(ms),
            Err(_) => {
                return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_mtime", "mtime must be milliseconds since the epoch."));
            }
        },
        None => None,
    };

    // The bundle must still be staged: uploads finish into it.
    let Some(bundle_dir) = state.paths.bundle(&bundle_id) else {
        return error_response(ApiErrorResponse::new(StatusCode::NOT_FOUND, "bundle_not_found", "The bundle does not exist."));
    };
    if !matches!(fs::metadata(&bundle_dir).await, Ok(meta) if meta.is_dir()) {
        return error_response(ApiErrorResponse::new(StatusCode::NOT_FOUND, "bundle_not_found", "The bundle does not exist."));
    }

    match crate::uploads::create(&state.paths, length, bundle_id, filename, mtime).await {
        Ok((id, _, _)) => {
            let mut response = empty_response(StatusCode::CREATED);
            if let Ok(location) = HeaderValue::from_str(&format!("/api/uploads/{id}")) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            response
        }
        Err(e) => error_response(
            ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_creation_failed", "Failed creating the upload files.").with_cause(&e.to_string()),
        ),
    }
}

#[utoipa::path(
    head,
    path = "/api/uploads/{upload_id}",
    description = "An upload's offset and length (tus core).",
    params(("upload_id" = String, Path, description = "Upload id from the Location header")),
    responses(
        (status = 200, description = "Upload-Offset and Upload-Length in the headers"),
        (status = 404, description = "Unknown or finished upload", body = ApiErrorBody),
    )
)]
pub async fn upload_status(State(state): State<AppState>, AxumPath(upload_id): AxumPath<String>) -> Response {
    let Ok(Some((_, info))) = crate::uploads::find(&state.paths, &upload_id).await else {
        return upload_not_found();
    };
    let mut response = empty_response(StatusCode::OK);
    let headers = response.headers_mut();
    headers.insert("upload-offset", HeaderValue::from(info.offset.max(0) as u64));
    headers.insert("upload-length", HeaderValue::from(info.length.max(0) as u64));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[utoipa::path(
    patch,
    path = "/api/uploads/{upload_id}",
    description = "Appends a chunk at Upload-Offset (tus core). The finished file joins the bundle.",
    request_body(content = Vec<u8>, content_type = "application/offset+octet-stream", description = "The bytes from Upload-Offset on"),
    params(
        ("upload_id" = String, Path, description = "Upload id from the Location header"),
        ("Tus-Resumable" = String, Header, description = "Must be 1.0.0"),
        ("Upload-Offset" = i64, Header, description = "Bytes already stored; must equal the server's offset"),
        ("Content-Type" = String, Header, description = "Must be application/offset+octet-stream"),
    ),
    responses(
        (status = 204, description = "Chunk stored; the new offset is in Upload-Offset"),
        (status = 400, description = "Bad offset or a body longer than Upload-Length", body = ApiErrorBody),
        (status = 404, description = "Unknown or finished upload", body = ApiErrorBody),
        (status = 409, description = "Upload-Offset doesn't match, or another PATCH is in progress", body = ApiErrorBody),
        (status = 412, description = "Tus-Resumable is missing or not 1.0.0", body = ApiErrorBody),
        (status = 415, description = "Content-Type is not application/offset+octet-stream", body = ApiErrorBody),
        (status = 500, description = "Failed writing the chunk", body = ApiErrorBody),
    )
)]
pub async fn upload_patch(
    State(state): State<AppState>,
    AxumPath(upload_id): AxumPath<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if header_str(&headers, "tus-resumable") != Some(TUS_VERSION) {
        return missing_tus_version();
    }
    let content_type = header_str(&headers, header::CONTENT_TYPE.as_str()).unwrap_or("");
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if media_type != OFFSET_CONTENT_TYPE {
        return error_response(ApiErrorResponse::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_content_type",
            "Content-Type must be application/offset+octet-stream.",
        ));
    }
    let requested = header_str(&headers, "upload-offset")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|offset| *offset >= 0);
    let Some(requested) = requested else {
        return error_response(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "upload_offset_required", "Upload-Offset is required."));
    };
    if UploadFiles::of(&state.paths, &upload_id).is_none() {
        return upload_not_found();
    }

    // One writer per upload: a second PATCH while the first streams gets a 409.
    let acquired = {
        let mut patches = state.upload_patches.lock().await;
        patches.insert(upload_id.clone())
    };
    if !acquired {
        return error_response(ApiErrorResponse::new(StatusCode::CONFLICT, "upload_in_progress", "Another PATCH for this upload is in progress."));
    }
    let response = patch_upload(&state, &upload_id, requested, body).await;
    state.upload_patches.lock().await.remove(&upload_id);
    response
}

async fn patch_upload(state: &AppState, upload_id: &str, requested: i64, body: Body) -> Response {
    let found = crate::uploads::find(&state.paths, upload_id).await;
    let Some((files, mut info)) = (match found {
        Ok(found) => found,
        Err(e) => {
            return error_response(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_read_failed", "Failed reading the upload.").with_cause(&e.to_string()));
        }
    }) else {
        return upload_not_found();
    };
    if requested != info.offset {
        return error_response(
            ApiErrorResponse::new(StatusCode::CONFLICT, "upload_offset_mismatch", "Upload-Offset doesn't match the server's offset.")
                .with_details(serde_json::json!({ "upload_offset": info.offset })),
        );
    }

    let mut file = match fs::OpenOptions::new().write(true).open(&files.part).await {
        Ok(file) => file,
        Err(e) => {
            return error_response(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Failed opening the upload file.").with_cause(&e.to_string()));
        }
    };
    if let Err(e) = file.seek(SeekFrom::Start(info.offset as u64)).await {
        return error_response(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Failed seeking in the upload file.").with_cause(&e.to_string()));
    }

    // Stream to disk: never buffer the whole chunk, never write past Upload-Length. The
    // offset is stored as chunks land, so a broken stream leaves a resumable upload.
    let mut offset = info.offset;
    let mut failure: Option<ApiErrorResponse> = None;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                failure = Some(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "upload_interrupted", "Reading the upload body failed.").with_cause(&e.to_string()));
                break;
            }
        };
        if offset + chunk.len() as i64 > info.length {
            failure = Some(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "upload_exceeds_length", "The body is longer than Upload-Length."));
            break;
        }
        if let Err(e) = file.write_all(&chunk).await {
            failure = Some(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Writing the upload failed.").with_cause(&e.to_string()));
            break;
        }
        offset += chunk.len() as i64;
        info.offset = offset;
        if let Err(e) = files.save(&info).await {
            failure = Some(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Saving the upload offset failed.").with_cause(&e.to_string()));
            break;
        }
    }
    if let Err(e) = file.flush().await
        && failure.is_none()
    {
        failure = Some(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Writing the upload failed.").with_cause(&e.to_string()));
    }
    drop(file);

    if let Some(failure) = failure {
        return error_response(failure);
    }
    if offset == info.length
        && let Err(error) = finish_upload(state, &files, &info).await
    {
        return error_response(error);
    }

    let mut response = empty_response(StatusCode::NO_CONTENT);
    response.headers_mut().insert("upload-offset", HeaderValue::from(offset as u64));
    response
}

#[utoipa::path(
    delete,
    path = "/api/uploads/{upload_id}",
    description = "Cancels an upload and drops its bytes (tus termination extension).",
    params(("upload_id" = String, Path, description = "Upload id from the Location header")),
    responses(
        (status = 204, description = "Upload removed"),
        (status = 404, description = "Unknown upload", body = ApiErrorBody),
        (status = 412, description = "Tus-Resumable is missing or not 1.0.0", body = ApiErrorBody),
    )
)]
pub async fn upload_terminate(State(state): State<AppState>, AxumPath(upload_id): AxumPath<String>, headers: HeaderMap) -> Response {
    if header_str(&headers, "tus-resumable") != Some(TUS_VERSION) {
        return missing_tus_version();
    }
    let Ok(Some((files, _))) = crate::uploads::find(&state.paths, &upload_id).await else {
        return upload_not_found();
    };
    match files.remove().await {
        Ok(()) => empty_response(StatusCode::NO_CONTENT),
        Err(e) => error_response(
            ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_delete_failed", "Failed deleting the upload.").with_cause(&e.to_string()),
        ),
    }
}

/// The upload is complete: its bytes join the bundle and its `meta.json`.
async fn finish_upload(state: &AppState, files: &UploadFiles, info: &UploadInfo) -> Result<(), ApiErrorResponse> {
    let Some(bundle_dir) = state.paths.bundle(&info.bundle_id) else {
        let _ = files.remove().await;
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "bundle_not_found", "The bundle this upload was for no longer exists."));
    };

    // Parallel files landing in one bundle: pick a free name and rewrite meta.json one at a time.
    let _guard = state.bundle_meta_lock.lock().await;
    if !matches!(fs::metadata(&bundle_dir).await, Ok(meta) if meta.is_dir()) {
        let _ = files.remove().await;
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "bundle_not_found", "The bundle this upload was for no longer exists."));
    }

    let name = unique_name(&bundle_dir, &info.filename).await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Failed naming the file in the bundle.").with_cause(&e.to_string())
    })?;
    if let Some(ms) = info.mtime
        && let Ok(mut file) = fs::OpenOptions::new().write(true).open(&files.part).await
    {
        crate::bundles::keep_original_date(&mut file, ms, &name).await;
    }
    let meta = crate::bundles::file_meta(files.part.clone(), &name).await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Failed reading the uploaded file.").with_cause(&e.to_string())
    })?;
    let mut bundle_meta = crate::bundles::read_bundle_meta(&bundle_dir).await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "meta_creation_failed", "Failed reading the bundle meta file.").with_cause(&e.to_string())
    })?;
    bundle_meta.files.push(meta);

    let destination = bundle_dir.join(&name);
    if let Err(e) = fs::rename(&files.part, &destination).await {
        return Err(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "upload_write_failed", "Moving the upload into the bundle failed.").with_cause(&e.to_string()));
    }
    if let Err(e) = crate::bundles::write_bundle_meta(&bundle_dir, bundle_meta.files).await {
        // Put the bytes back so the client can retry the final PATCH.
        let _ = fs::rename(&destination, &files.part).await;
        return Err(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "meta_creation_failed", "Failed writing the bundle meta file.").with_cause(&e.to_string()));
    }
    let _ = fs::remove_file(&files.info).await;
    Ok(())
}

/// `name`, or `name (2)`, `name (3)`… if the bundle already has that file.
async fn unique_name(dir: &Path, name: &str) -> std::io::Result<String> {
    let mut n = 1u64;
    loop {
        let candidate = if n == 1 { name.to_string() } else { numbered(name, n) };
        if !fs::try_exists(dir.join(&candidate)).await? {
            return Ok(candidate);
        }
        n += 1;
    }
}

fn numbered(name: &str, n: u64) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({n}).{extension}"),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::{numbered, parse_metadata};

    #[test]
    fn metadata_values_are_base64() {
        // "bundle-id", "benchy.stl", "1700000000000"
        let raw = "bundle_id YnVuZGxlLWlk,filename YmVuY2h5LnN0bA==,mtime MTcwMDAwMDAwMDAwMA==";
        let fields = parse_metadata(raw).unwrap();
        assert_eq!(fields["bundle_id"], "bundle-id");
        assert_eq!(fields["filename"], "benchy.stl");
        assert_eq!(fields["mtime"], "1700000000000");
        assert!(parse_metadata("filename !!!").is_err());
    }

    #[test]
    fn duplicate_names_get_a_number() {
        assert_eq!(numbered("benchy.stl", 2), "benchy (2).stl");
        assert_eq!(numbered("benchy.stl", 3), "benchy (3).stl");
        assert_eq!(numbered("README", 2), "README (2)");
        assert_eq!(numbered(".hidden", 2), ".hidden (2)");
        assert_eq!(numbered("a.b.stl", 2), "a.b (2).stl");
    }
}
