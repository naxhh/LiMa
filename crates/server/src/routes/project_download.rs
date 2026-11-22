//! `GET /api/projects/{id}/download`: every indexed file of a project as one zip, streamed.
//! `POST /api/projects/download`: several projects in one zip, one folder each.

use std::io::{self, Write};
use std::path::{Component, Path as FsPath, PathBuf};

use axum::{
    Form,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use lima_db::queries::projects_detail::GetProjectError;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

/// Bytes buffered between the zip thread and the HTTP body.
const PIPE_BUFFER: usize = 256 * 1024;

/// Extensions that are already compressed; storing them avoids wasting CPU.
const STORED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "avif", "heic", "zip", "7z", "rar", "gz", "xz", "zst", "3mf", "mp4", "mov",
    "webm", "pdf",
];

#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/download",
    tag = "Projects",
    params(("project_id" = String, Path, description = "Project id")),
    responses(
        (status = 200, description = "Zip archive of the project's files", content_type = "application/zip"),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn project_download(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Response, ApiErrorResponse> {
    let pool = state.db.pool();
    let project = lima_db::queries::projects_detail::get_project(pool, &project_id).await.map_err(|e| match e {
        GetProjectError::NotFound => {
            ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "Project not found")
        }
        GetProjectError::Db(e) => {
            ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
        }
    })?;
    let assets = lima_db::queries::projects_detail::get_project_assets(pool, &project_id).await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
    })?;

    let root = state.paths.project_dir(&project.folder_path);
    let entries: Vec<(String, PathBuf, u64)> = assets
        .into_iter()
        .filter_map(|a| {
            let rel = safe_relative(&a.file_path)?;
            Some((rel.to_string_lossy().replace('\\', "/"), root.join(&rel), a.size_bytes.max(0) as u64))
        })
        .collect();

    Ok(stream_zip(entries, archive_name(&project.folder_path), project.folder_path.clone()))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct DownloadManyForm {
    /// Comma-separated project ids.
    pub ids: String,
}

/// Most projects in one multi-project zip.
const MAX_ZIP_PROJECTS: usize = 500;

#[utoipa::path(
    post,
    path = "/api/projects/download",
    tag = "Projects",
    request_body(content = DownloadManyForm, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Zip with one folder per project", content_type = "application/zip"),
        (status = 400, description = "No ids, or too many", body = ApiErrorBody),
        (status = 503, description = "Database failure", body = ApiErrorBody),
    )
)]
pub async fn download_many(State(state): State<AppState>, Form(form): Form<DownloadManyForm>) -> Result<Response, ApiErrorResponse> {
    let ids: Vec<&str> = form.ids.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if ids.is_empty() || ids.len() > MAX_ZIP_PROJECTS {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_ids", "Send between 1 and 500 project ids"));
    }
    let pool = state.db.pool();
    let db_error = |e: sqlx::Error| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string());
    let mut entries = Vec::new();
    for id in &ids {
        // Missing projects are skipped.
        let project = match lima_db::queries::projects_detail::get_project(pool, id).await {
            Ok(p) => p,
            Err(GetProjectError::NotFound) => continue,
            Err(GetProjectError::Db(e)) => return Err(db_error(e)),
        };
        let Some(folder) = safe_relative(&project.folder_path) else { continue };
        let root = state.paths.project_dir(&project.folder_path);
        for a in lima_db::queries::projects_detail::get_project_assets(pool, id).await.map_err(db_error)? {
            if let Some(rel) = safe_relative(&a.file_path) {
                let name = folder.join(&rel).to_string_lossy().replace('\\', "/");
                entries.push((name, root.join(&rel), a.size_bytes.max(0) as u64));
            }
        }
    }
    let name = format!("lima-{}-projects.zip", ids.len());
    Ok(stream_zip(entries, name, format!("{} projects", ids.len())))
}

/// Streams a zip of `entries` (archive path, file on disk, size) as the response body.
fn stream_zip(entries: Vec<(String, PathBuf, u64)>, name: String, what: String) -> Response {
    let (reader, writer) = tokio::io::duplex(PIPE_BUFFER);
    tokio::task::spawn_blocking(move || {
        let sink = SyncIoBridge::new(writer);
        if let Err(e) = write_zip(sink, &entries) {
            // The client sees a truncated download; nothing else we can do mid-stream.
            tracing::warn!("Zip download of {what} failed: {e}");
        }
    });

    let mut res = Response::new(Body::from_stream(ReaderStream::new(reader)));
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition(&name));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn write_zip<W: Write>(sink: W, entries: &[(String, PathBuf, u64)]) -> io::Result<()> {
    let mut zip = ZipWriter::new_stream(sink);
    for (name, path, size) in entries {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            // Deleted since the last sync: skip it rather than failing the whole archive.
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let method = if is_stored(name) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        let mut options = SimpleFileOptions::default().compression_method(method).large_file(*size >= u32::MAX as u64);
        if let Some(t) = file.metadata().ok().and_then(|m| m.modified().ok()).and_then(zip_time) {
            options = options.last_modified_time(t);
        }
        zip.start_file(name, options).map_err(io::Error::other)?;
        io::copy(&mut file, &mut zip)?;
    }
    zip.finish().map_err(io::Error::other)?;
    Ok(())
}

/// File modification time as a zip timestamp (UTC; zip has no time zones). `None` before 1980.
fn zip_time(t: std::time::SystemTime) -> Option<zip::DateTime> {
    let t = time::OffsetDateTime::from(t);
    zip::DateTime::from_date_and_time(
        u16::try_from(t.year()).ok()?,
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    )
    .ok()
}

fn is_stored(name: &str) -> bool {
    FsPath::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| STORED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// `file_path` from the database as a plain relative path, or `None` if it tries to escape.
fn safe_relative(file_path: &str) -> Option<PathBuf> {
    let p = FsPath::new(file_path);
    let clean: PathBuf = p.components().filter(|c| !matches!(c, Component::CurDir)).collect();
    let ok = !clean.as_os_str().is_empty() && clean.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then_some(clean)
}

/// Last path segment of the project folder, used as the zip name.
fn archive_name(folder_path: &str) -> String {
    let base = folder_path.trim_end_matches('/').rsplit('/').next().unwrap_or("project");
    let base = if base.is_empty() { "project" } else { base };
    format!("{base}.zip")
}

/// `attachment` with an ASCII fallback plus the exact UTF-8 name (RFC 6266 / 5987).
fn content_disposition(name: &str) -> HeaderValue {
    let ascii: String =
        name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    HeaderValue::from_str(&format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"project.zip\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape() {
        assert_eq!(safe_relative("a/b.stl"), Some(PathBuf::from("a/b.stl")));
        assert_eq!(safe_relative("./a.stl"), Some(PathBuf::from("a.stl")));
        assert_eq!(safe_relative("../x"), None);
        assert_eq!(safe_relative("a/../../x"), None);
        assert_eq!(safe_relative("/etc/passwd"), None);
        assert_eq!(safe_relative(""), None);
    }

    #[test]
    fn names_and_headers() {
        assert_eq!(archive_name("desk-dragon"), "desk-dragon.zip");
        assert_eq!(archive_name("a/b/"), "b.zip");
        let h = content_disposition("Sin título.zip");
        assert_eq!(h.to_str().unwrap(), "attachment; filename=\"Sin t_tulo.zip\"; filename*=UTF-8''Sin%20t%C3%ADtulo.zip");
        assert!(is_stored("x/photo.JPG"));
        assert!(!is_stored("model.stl"));
    }

    #[test]
    fn zip_roundtrip_skips_missing_files() {
        let dir = std::env::temp_dir().join(format!("lima-zip-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.stl"), b"solid a").unwrap();
        std::fs::write(dir.join("sub/b.jpg"), b"jpeg").unwrap();
        let entries = vec![
            ("a.stl".to_string(), dir.join("a.stl"), 7),
            ("gone.stl".to_string(), dir.join("gone.stl"), 1),
            ("sub/b.jpg".to_string(), dir.join("sub/b.jpg"), 4),
        ];
        let mut out = Vec::new();
        write_zip(&mut out, &entries).unwrap();
        // A valid zip ends with the end-of-central-directory signature and lists both names.
        assert!(out.windows(4).any(|w| w == b"PK\x05\x06"));
        assert!(out.windows(5).any(|w| w == b"a.stl"));
        assert!(out.windows(9).any(|w| w == b"sub/b.jpg"));
        assert!(!out.windows(8).any(|w| w == b"gone.stl"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
