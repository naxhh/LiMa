use axum::{ http::StatusCode, extract::{Path, State}, Json };
use serde::Serialize;
use utoipa::ToSchema;
use lima_db::queries::projects_detail::{ProjectTagRow, ProjectAssetRow, GetProjectError};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[derive(Serialize, ToSchema)]
pub struct ProjectDetailResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub folder_path: String,
    pub main_image_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_scanned_at: Option<String>,

    pub assets : Vec<ProjectAssetResponse>,
    pub tags : Vec<ProjectTagResponse>,
    // TODO: add collections?
}

#[derive(Serialize, ToSchema)]
pub struct ProjectAssetResponse {
    pub id: String,
    pub file_path: String,
    pub kind: String,
    pub size_bytes: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectTagResponse {
    pub id: String,
    pub name: String,
    pub color: String,
    
}

#[utoipa::path(
    get,
    path = "/api/projects/{project_id}",
    params(
        ("project_id" = String, Path, description = "The ID of the project to retrieve"),
    ),
    responses(
        (status = 200, description = "Project details retrieved successfully", body = ProjectDetailResponse),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 500, description = "Internal server error", body = ApiErrorBody),
    )
)]
pub async fn project_detail(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<(StatusCode, Json<ProjectDetailResponse>), ApiErrorResponse> {

    let project = match lima_db::queries::projects_detail::get_project(
        state.db.pool(),
        &project_id,        
    ).await {
        Ok(proj) => proj,
        Err(GetProjectError::NotFound) => {
            return Err(ApiErrorResponse::new(
                StatusCode::NOT_FOUND,
                "project_not_found",
                "Project not found",
            ));
        }
        Err(e) => {
            return Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal server error occurred",
            ).with_cause(&e.to_string()));
        }
    };

    let project_tags = match lima_db::queries::projects_detail::get_project_tags(
        state.db.pool(),
        &project_id,
    ).await {
        Ok(tags) => tags,
        Err(e) => {
            return Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal server error occurred",
            ).with_cause(&e.to_string()));
        }
    };

    let project_assets = match lima_db::queries::projects_detail::get_project_assets(
        state.db.pool(),
        &project_id,
    ).await {
        Ok(assets) => assets,
        Err(e) => {
            return Err(ApiErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal server error occurred",
            ).with_cause(&e.to_string()));
        }
    };

    Ok((
        StatusCode::OK,
        Json(ProjectDetailResponse {
            id: project.id,
            name: project.name,
            description: project.description,
            folder_path: project.folder_path,
            main_image_id: project.main_image_id,
            created_at: project.created_at,
            updated_at: project.updated_at,
            last_scanned_at: project.last_scanned_at,
            assets: map_assets(project_assets),
            tags: map_tags(project_tags),
        }),
    ))

}

fn map_assets(db_assets: Vec<ProjectAssetRow>) -> Vec<ProjectAssetResponse> {
    db_assets.into_iter().map(|asset| {
        ProjectAssetResponse {
            id: asset.id,
            file_path: asset.file_path,
            kind: asset.kind,
            size_bytes: asset.size_bytes,
        }
    }).collect()
}

fn map_tags(db_tags: Vec<ProjectTagRow>) -> Vec<ProjectTagResponse> {
    db_tags.into_iter().map(|tag| {
        ProjectTagResponse {
            id: tag.id,
            name: tag.name,
            color: tag.color,
        }
    }).collect()
}