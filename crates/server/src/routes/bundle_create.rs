use axum::{Json, extract::{Multipart, State}, http::StatusCode};
use mime_guess::MimeGuess;
use serde::{Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use std::{path::PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use utoipa::ToSchema;
use std::io::{Error, ErrorKind};
use lima_domain::models::bundle::{BundleMeta, FileMeta};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[derive(Serialize, ToSchema)]
pub struct CreateBundleResponse {
    pub id: String,
    pub files: Vec<String>,
    pub failed_files: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/bundles",
    request_body(
        content_type = "multipart/form-data",
        description = "Set of files to upload using files or files[] fields."
    ),
    responses(
        (status = 201, description = "Bundle created. Some files may have failed check payload", body = CreateBundleResponse),
        (status = 400, description = "Errors on the received files", body = ApiErrorBody),
        (status = 500, description = "Failed creating files or folders", body = ApiErrorBody),
    )
)]
pub async fn create_bundle(
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CreateBundleResponse>), ApiErrorResponse> {
    // TODO: move logic to db module
    let bundle_id = Uuid::new_v4().to_string();
    let bundle_folder: PathBuf = ["data", "state", "bundles", &bundle_id].iter().collect();
    let mut failed_files: Vec<String> = Vec::new();
    let mut files_metadata: Vec<FileMeta> = Vec::new();

    fs::create_dir_all(&bundle_folder).await.map_err(|e| {
        tracing::error!("Failed to create bundle directory: {}", e);
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "dir_creation_failed",
            "Failed to create bundle directory"
        ).with_cause(&e.to_string())
    })?;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("bundle multipart next_field error: {e}");
        ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "invalid_multipart",
            "Invalid multipart data"
        ).with_cause(&e.to_string())
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name != "files" && field_name != "files[]" {
            tracing::debug!("Skipping unexpected field: {}", field_name);
            continue;
        }

        let raw_name = field.file_name().unwrap_or("").to_string();
        let file_name = match field.file_name()
            .and_then(sanitize_filename) {
                Some(name ) => name,
                None => {
                    tracing::error!("Invalid filename in uploaded bundle: {}", raw_name);
                    failed_files.push(raw_name);
                    continue;
                }
            };

        
        let file_destination = bundle_folder.join(&file_name);
        let mut file = match fs::File::create(&file_destination).await {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Failed to create file {}: {}", file_destination.display(), e);
                failed_files.push(file_name);
                continue;
            }
        };

        let mut size: i64 = 0;
        let mut field = field;

        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as i64;
                    if let Err(e) = file.write_all(&chunk).await {
                        tracing::error!("Failed to write to file {}: {}", file_destination.display(), e);
                        failed_files.push(file_name);
                        let _ = fs::remove_file(&file_destination).await;
                        break;
                    }
                }
                Ok(None) => {
                    tracing::debug!("Uploaded file: {}, dst: {}, size: {}", file_name, file_destination.display(), size);

                    files_metadata.push(FileMeta {
                        name: file_name.clone(),
                        size,
                        mtime: extract_mtime(&file).await.ok(),
                        mime: guess_mime(&file_name),
                        kind: extract_kind(&file_name).to_string(),
                        checksum: compute_checksum(&file_destination).await.ok(),
                    });
                    break;
                },
                Err(e) => {
                    tracing::error!("error reading chunk for file {}: {}", file_name, e);
                    failed_files.push(file_name);
                    let _ = fs::remove_file(&file_destination).await;
                    break;
                }
            }
        }
    }

    if files_metadata.is_empty() {
        let _ = fs::remove_dir_all(&bundle_folder).await;

        return Err(ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "no_valid_files",
            "No valid files were uploaded in the bundle"
        ));
    }

    write_bundle_meta(
        &bundle_folder,
        files_metadata.clone(),
    ).await.map_err(|e| {
        tracing::error!("Failed creating the meta file for {} bundle", bundle_id);
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "meta_creation_failed",
            "Failed creating the bundle meta file"
        ).with_cause(&e.to_string())
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateBundleResponse {
            id: bundle_id,
            files: files_metadata.iter().map(|f| f.name.clone()).collect(),
            failed_files: failed_files,
        }),
    ))
}

fn sanitize_filename(name: &str) -> Option<String> {
    if name.is_empty() {
        tracing::error!("empty file name");
        return None;
    }

    if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        tracing::error!("file name contains invalid characters: {}", name);
        return None;
    }

    Some(name.to_string())
}

pub async fn write_bundle_meta(
    bundle_dir: &PathBuf,
    files_meta: Vec<FileMeta>,
) -> Result<(), Error> {
    let bundle_meta = BundleMeta {
        uploaded_at: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_string()),
        files: files_meta,
    };


    let meta_path = bundle_dir.join("meta.json");
    let json = serde_json::to_vec_pretty(&bundle_meta)
        .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

    let mut file = fs::File::create(&meta_path).await?;
    file.write_all(&json).await?;
    file.flush().await?;
    drop(file);

    Ok(())
}

// TODO: move to models and enum so we keep track of the support we provide.
fn extract_kind(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".webp") {
        "image"
    } else if lower.ends_with(".stl") || lower.ends_with(".obj") || lower.ends_with(".3mf") || lower.ends_with(".fbx") || lower.ends_with(".glb") || lower.ends_with(".gltf") {
        "model"
    } else {
        "other"
    }
}

fn guess_mime(path: &str) -> String {
    MimeGuess::from_path(path)
    .first_or_octet_stream()
    .to_string()
}

async fn extract_mtime(file: &fs::File) -> Result<String, Error> {
    let metadata = file.metadata().await?;

    OffsetDateTime::from(metadata.modified()?)
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| {
            Error::new(ErrorKind::Other, format!("Failed to format mtime: {}", e))
        })
}

async fn compute_checksum(path: &PathBuf) -> Result<String, Error> {
    use tokio::{fs::File, io::AsyncReadExt};
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();
    let mut reader = File::open(path).await?;
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}