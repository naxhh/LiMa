use axum::{Json, extract::{Multipart, State}, http::StatusCode};
use serde::Serialize;
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
    path = "/api/bundles",
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
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CreateBundleResponse>), ApiErrorResponse> {
    let mut bundle = crate::bundles::NewBundle::create(&state.paths).await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "dir_creation_failed", "Failed to create bundle directory").with_cause(&e.to_string())
    })?;
    // Optional `mtime` field (ms since the epoch) right before a file: its original date.
    let mut next_mtime: Option<u64> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_multipart", "Invalid multipart data").with_cause(&e.to_string())
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "mtime" {
            next_mtime = field.text().await.ok().and_then(|t| t.trim().parse::<u64>().ok());
            continue;
        }
        let mtime = next_mtime.take();
        if field_name != "files" && field_name != "files[]" {
            continue;
        }
        let raw_name = field.file_name().unwrap_or("").to_string();
        let Some(mut writer) = bundle.start_file(&raw_name).await else { continue };
        let result = loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if let Err(e) = writer.write(&chunk).await {
                        break Err(e.to_string());
                    }
                }
                Ok(None) => break Ok(()),
                Err(e) => break Err(e.to_string()),
            }
        };
        bundle.end_file(writer, result, mtime).await;
    }

    match bundle.finish().await {
        Ok(Some((id, files, failed_files))) => Ok((StatusCode::CREATED, Json(CreateBundleResponse { id, files, failed_files }))),
        Ok(None) => Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "no_valid_files", "No valid files were uploaded in the bundle")),
        Err(e) => Err(ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "meta_creation_failed", "Failed creating the bundle meta file").with_cause(&e.to_string())),
    }
}
