use axum::{extract::{State}, http::StatusCode, Json};
use serde::{Serialize, Deserialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateProjectResponse {
    pub id: String,
    pub folder_path: String,
}

#[utoipa::path(
    post,
    path = "/projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "Project created", body = CreateProjectResponse),
        (status = 409, description = "Project with same name or path already exists"),
        (status = 503, description = "Failure to connect to the database"),
    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<CreateProjectResponse>), StatusCode> {
    // TODO: we probably should create & check folders can be created for the project here

    let name = body.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let description = body.description.unwrap_or_default();
    let tags = body.tags.unwrap_or_default();
    let now = now();

    match lima_db::queries::projects_create::create_project(
        state.db.pool(),
        name,
        &description,
        None,
        &tags,
        &now,
    )
    .await {
        Ok(created) => {
            tracing::info!("Project created with id {} & path: {}", created.id, created.folder_path);
            return Ok((
                StatusCode::CREATED,
                Json(CreateProjectResponse {
                    id: created.id,
                    folder_path: created.folder_path,
                }),
            ));
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!("Failed to create project: {}", msg);
            if msg.contains("UNIQUE") || msg.contains("unique") {
                return Err(StatusCode::CONFLICT);
            } else {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
        }
    }


}

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}