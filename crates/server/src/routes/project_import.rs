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
}

#[utoipa::path(
    post,
    path = "/projects/{project_id}/import",
    request_body = ImportProjectRequest,
    params(
        ("project_id" = String, Path, description = "The ID of the project to import assets into"),
    ),
    responses(
        (status = 200, description = "Project imported successfully"),
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
) -> Result<StatusCode, ApiErrorResponse> {
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

    let assets = lima_db::queries::projects_import::import_assets_from_bundle(
        app_state.db.pool(),
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

    Ok(StatusCode::OK)
}
