use axum::{
    extract::{Multipart, State, Path},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::path::{PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct UploadAssetsResponse {
    pub project_id: String,
    pub added: usize,
    pub assets: Vec<UploadedAsset>,
}

#[derive(Serialize, ToSchema)]
pub struct UploadedAsset {
    pub id: String,
    pub file_path: String,
    pub kind: String,
}

#[utoipa::path(
    post,
    path = "/projects/{project_id}/assets",
    request_body(
        content_type = "multipart/form-data",
        description = "Files to upload as assets to the project. Use 'files' or 'files[]' as field names. Optionally, include a 'main_image' field with the filename of the main image asset."
    ),
    responses(
        (status = 201, description = "Assets uploaded", body = UploadAssetsResponse),
        (status = 409, description = "Assets with same names exist in the project"),
        (status = 503, description = "Failure to connect to the database"),
    )
)]
pub async fn upload_assets(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadAssetsResponse>), StatusCode> {
    let folder_path = lima_db::queries::assets::get_project_folder_path(&state.db.pool(), &project_id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let upload_id = uuid::Uuid::new_v4().to_string();
    // TODO: i may want to use the actual TMP path.
    let staging_dir: PathBuf = ["data", "state", "uploads", &upload_id].iter().collect();
    fs::create_dir_all(&staging_dir).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut staged: Vec<(String, PathBuf, i64)> = Vec::new(); // (filename, path, size)
    let mut main_image: Option<String> = None;

    tracing::debug!("Staging upload will be: {}", staging_dir.display());

    // Upload to temporary folder first so we don't pollute the data folder with wrong uploads.
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("multipart next_field error: {e}");
        StatusCode::BAD_REQUEST
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "main_image" {
            main_image = Some(field.text().await.map_err(|e| {
                tracing::error!("main_image error: {e}");
                StatusCode::BAD_REQUEST
            })?);
            tracing::debug!("Main image requested: {}", main_image.as_deref().unwrap_or(""));
            continue;
        }


        if field_name != "files" && field_name != "files[]" {
            // Ignore unknown fields
            tracing::debug!("Unkwnown fields: {}", field_name);
            continue;
        }

        let filename = field
            .file_name()
            .and_then(|s| sanitize_filename(s))
            .ok_or(StatusCode::BAD_REQUEST)?;

        let destination = staging_dir.join(&filename);
        let mut file = fs::File::create(&destination).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut size: i64 = 0;
        let mut field = field;
        while let Some(chunk) = field.chunk().await.map_err(|e| {
            tracing::error!("error reading chunk for file {}: {}", filename, e);
            StatusCode::BAD_REQUEST
        })? {
            size += chunk.len() as i64;
            file.write_all(&chunk)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }

        tracing::debug!("Staged file: {}, dst: {}, size: {}", filename, destination.display(), size);
        staged.push((filename, destination, size));
    }

    // Move uploaded files to the final destination.
    let project_dir: PathBuf = ["data", "library", &folder_path].iter().collect();
    fs::create_dir_all(&project_dir).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::debug!("Project directory will be: {}", project_dir.display());

    let mut moved: Vec<(String, PathBuf, i64)> = Vec::new(); // (filename, path, size)
    for (filename, source, size) in staged {
        let destination = project_dir.join(&filename);
        
        if fs::try_exists(&destination).await.unwrap_or(false) {
            // Cleanup duplicate file.
            let _ = fs::remove_dir_all(&staging_dir).await;
            return Err(StatusCode::CONFLICT);
        }

        tracing::debug!("Renaming file: {}, dst: {}, size: {}", &source.display(), destination.display(), size);
        fs::rename(&source, &destination).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        moved.push((filename, destination, size));
        // TODO: rework this more. I want to update DB first and move files after.
    }

    let now = now();
    let mtime = now.as_str(); // TODO: this should use fs metadata

    let mut tx = state.db.pool().begin().await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut assets_out = Vec::new();

    for (filename, _dst, size) in &moved {
        let kind = extract_kind(filename);
        let mime = ""; // TODO: detect

        let inserted = lima_db::queries::assets::insert_asset(
            &mut tx,
            &project_id,
            &filename,
            kind,
            *size,
            mtime,
            mime,
            &now,
        ).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;


        tracing::debug!("Asset uploaded: {}, dst: {}, kind: {}", inserted.id, inserted.file_path, inserted.kind);
        assets_out.push(UploadedAsset {
            id: inserted.id,
            file_path: inserted.file_path,
            kind: inserted.kind,
        });
    }

    if let Some(main_image) = main_image.as_deref() {
        if let Some(uploaded_asset) = assets_out.iter().find(|asset| asset.file_path == main_image) {
            tracing::debug!("Setting main image: {} for project: {}", main_image, project_id);
            lima_db::queries::assets::set_project_main_image(&mut tx, &project_id, &uploaded_asset.id, &now)
            .await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        }
    }

    tx.commit().await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let _ = fs::remove_dir_all(&staging_dir).await;

    Ok((
        StatusCode::CREATED,
        Json(UploadAssetsResponse {
            project_id,
            added: assets_out.len(),
            assets: assets_out,
        }),
    ))
}

// TODO: done twice extract away
fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap()
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

fn sanitize_filename(name: &str) -> Option<String> {
    if name.is_empty() {
        tracing::error!("empty asset name");
        return None;
    }

    if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        tracing::error!("asset name contains invalid characters: {}", name);
        return None;
    }

    Some(name.to_string())
}