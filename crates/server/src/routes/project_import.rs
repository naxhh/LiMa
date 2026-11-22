use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use utoipa::ToSchema;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use lima_db::queries::projects_detail::GetProjectError;
use lima_db::queries::projects_import::ImportFromBundleError;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};


#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct ImportProjectRequest {
    pub bundle_id: String,
    pub new_main_image: Option<String>,
    /// Extract zip files into the project, keeping their folders. Default false.
    #[serde(default)]
    pub extract_zips: bool,
    /// With `extract_zips`, also keep the original archives. Default false.
    #[serde(default)]
    pub keep_zips: bool,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct ImportProjectResponse {
    /// Files added to the project.
    pub imported: usize,
    /// What extracting zips did (all zero when `extract_zips` was off or there were none).
    pub extracted: crate::archive::ExtractSummary,
}

/// Extracts the bundle's zips in place, before its files are moved into the project.
pub(crate) async fn extract_zips(state: &AppState, project_id: &str, bundle_id: &str, keep_zips: bool) -> Result<crate::archive::ExtractSummary, ApiErrorResponse> {
    let Some(dir) = state.paths.bundle(bundle_id) else {
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "bundle_not_found", "Bundle not found."));
    };
    let existing: Vec<String> = lima_db::queries::projects_detail::get_project_assets(state.db.pool(), project_id)
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "database_error", "Failed to read the project's files.").with_cause(&e.to_string()))?
        .into_iter()
        .map(|a| a.file_path)
        .collect();
    crate::archive::extract_bundle_zips(&dir, &existing, keep_zips).await.map_err(|e| match e {
        crate::archive::ExtractError::Archive { name, error: lima_indexer::archive::ArchiveError::TooLarge } => {
            ApiErrorResponse::new(StatusCode::PAYLOAD_TOO_LARGE, "zip_too_large", "A zip file is too large to extract.").with_cause(&name)
        }
        crate::archive::ExtractError::Archive { name, error: lima_indexer::archive::ArchiveError::Io(e) } => {
            ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "filesystem_error", "Extracting a zip file failed.").with_cause(&format!("{name}: {e}"))
        }
        crate::archive::ExtractError::Archive { name, error } => {
            ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_zip", "A zip file couldn't be read.").with_cause(&format!("{name}: {error}"))
        }
        crate::archive::ExtractError::Io(e) => {
            ApiErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "filesystem_error", "Extracting zip files failed.").with_cause(&e)
        }
    })
}

#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/import",
    request_body = ImportProjectRequest,
    params(
        ("project_id" = String, Path, description = "The ID of the project to import assets into"),
    ),
    responses(
        (status = 200, description = "Files imported", body = ImportProjectResponse),
        (status = 413, description = "A zip is too large to extract (limits: 10,000 files, 8 GiB)", body = ApiErrorBody),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 404, description = "Project or bundle not found", body = ApiErrorBody),
        (status = 409, description = "Conflict during import", body = ApiErrorBody),
        (status = 412, description = "Precondition failed", body = ApiErrorBody),
        (status = 500, description = "Internal server error", body = ApiErrorBody),
        (status = 503, description = "Service unavailable", body = ApiErrorBody),
    )
)]
pub async fn project_import(
    State(app_state): State<AppState>,
    Path(project_id): Path<String>,
    Json(payload): Json<ImportProjectRequest>,
) -> Result<Json<ImportProjectResponse>, ApiErrorResponse> {
    if payload.bundle_id.is_empty() {
        return Err(ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "missing_fields",
            "Bundle ID must be provided for import.",
        ));
    }

    let project = match lima_db::queries::projects_detail::get_project(
        app_state.db.pool(),
        &project_id,
    ).await {
        Ok(project) => project,
        Err(GetProjectError::NotFound) => {
            return Err(ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found.",
            ));
        }
        Err(e) => {
            return Err(ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                "Failed to retrieve project details.",
            ).with_cause(&e.to_string()));
        }
    };

    let extracted = if payload.extract_zips {
        extract_zips(&app_state, &project.id, &payload.bundle_id, payload.keep_zips).await?
    } else {
        crate::archive::ExtractSummary::default()
    };

    let assets = lima_db::queries::projects_import::import_assets_from_bundle(
        app_state.db.pool(),
        &app_state.paths,
        &project.id,
        &payload.bundle_id,
    ).await.map_err(|e| { match e {
        ImportFromBundleError::BundleNotFound => {
            ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "bundle_not_found",
                "Bundle not found.",
            )
        },
        ImportFromBundleError::ProjectNotFound => {
            ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found.",
            )
        },
        ImportFromBundleError::MetaNotFound => {
            ApiErrorResponse::new(
                StatusCode::PRECONDITION_FAILED,
                "meta_not_found",
                "Bundle has an invalid format, meta file missing.",
            )
        },
        ImportFromBundleError::MissingFile { name } => {
            ApiErrorResponse::new(
                StatusCode::PRECONDITION_FAILED,
                "missing_file",
                "Bundle has an invalid format",
            ).with_cause(&format!("missing file: {}", name))
        },
        ImportFromBundleError::Conflict { name } => {
            ApiErrorResponse::new(
                StatusCode::CONFLICT,
                "file_conflict",
                "Import would overwrite existing file",
            ).with_cause(&format!("file: {}", name))
        },
        ImportFromBundleError::FileSystemError(msg) => {
            ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "filesystem_error",
                "A file system error occurred during import.",
            ).with_cause(&msg)
        },
        ImportFromBundleError::DatabaseError(err) => {
            ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                "A database error occurred during import.",
            ).with_cause(&err.to_string())
        },

    }})?;

    generate_thumbnails(&app_state, &project.id, &project.folder_path, &assets).await;

    let now = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "time_format_error",
            "Failed to format current time: {}",
        ).with_cause(&e.to_string())
    })?;


    // update main image if given or no main image exists
    if project.main_image_id.is_none() {
        // get first image in assets
        if let Some(first_image) = assets.iter().find(|a| a.kind == "image") {
            let _ = lima_db::queries::projects_update::set_main_image(
                app_state.db.pool(),
                &project.id,
                &first_image.id,
                now.as_str(),
            ).await.map_err(|e| {
                ApiErrorResponse::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "database_error",
                    "Failed to set project main image.",
                ).with_cause(&e.to_string())
            })?;
        }
    } else if let Some(ref main_image) = payload.new_main_image {
        let image_asset = assets.iter().find(|a| a.file_path == *main_image);
        let image_asset = match image_asset {
            Some(asset) => asset,
            None => {
                return Err(ApiErrorResponse::new(
                StatusCode::BAD_REQUEST,
                "invalid_main_image",
                "The specified main image does not exist in the imported assets.",
            ));
            },
        };

        let _ = lima_db::queries::projects_update::set_main_image(
            app_state.db.pool(),
            &project.id,
            image_asset.id.as_ref(),
            now.as_str(),
        ).await.map_err(|e| {
            ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                "Failed to set project main image.",
            ).with_cause(&e.to_string())
        })?;
        
    }

    Ok(Json(ImportProjectResponse { imported: assets.len(), extracted }))
}

/// Thumbnails for freshly imported images and models. Failures are only logged: the next
/// sync retries any image without a thumbnail.
pub(crate) async fn generate_thumbnails(
    state: &AppState,
    project_id: &str,
    folder_path: &str,
    assets: &[lima_db::queries::projects_detail::ProjectAssetRow],
) {
    let jobs: Vec<(std::path::PathBuf, std::path::PathBuf)> = assets
        .iter()
        .filter(|a| lima_indexer::thumbnails::has_thumbnail(std::path::Path::new(&a.file_path), &a.kind))
        .map(|a| {
            (
                state.paths.project_dir(folder_path).join(&a.file_path),
                lima_indexer::thumbnails::thumb_path(&state.paths.thumbnails(), project_id, &a.id),
            )
        })
        .collect();
    if jobs.is_empty() {
        return;
    }

    let result = tokio::task::spawn_blocking(move || {
        for (source, dest) in jobs {
            if let Err(e) = lima_indexer::thumbnails::generate_any(&source, &dest) {
                tracing::warn!("Thumbnail generation failed for {}: {e}", source.display());
            }
        }
    })
    .await;
    if let Err(e) = result {
        tracing::error!("Thumbnail task crashed: {e}");
    }
}
