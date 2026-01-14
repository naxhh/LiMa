use axum::{Json, extract::{Multipart, State}, http::StatusCode};
use serde::{Serialize};
use uuid::Uuid;
use std::path::PathBuf;
use tokio::{fs, io::AsyncWriteExt};
use utoipa::ToSchema;

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
    let bundle_id = Uuid::new_v4().to_string();
    let bundle_folder: PathBuf = ["data", "state", "bundles", &bundle_id].iter().collect();
    let mut uploaded_files: Vec<String> = Vec::new();
    let mut failed_files: Vec<String> = Vec::new();

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
                    uploaded_files.push(file_name);
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

    if uploaded_files.is_empty() {
        let _ = fs::remove_dir_all(&bundle_folder).await;

        return Err(ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "no_valid_files",
            "No valid files were uploaded in the bundle"
        ));
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateBundleResponse {
            id: bundle_id,
            files: uploaded_files,
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