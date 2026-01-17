use axum::{extract::{State}, http::StatusCode, Json};
use serde::{Serialize, Deserialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use utoipa::ToSchema;

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

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
        (status = 409, description = "Project with same name or path already exists", body = ApiErrorBody),
        (status = 503, description = "Failure to connect to the database", body = ApiErrorBody),
    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<CreateProjectResponse>), ApiErrorResponse> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "empty_name", "Project name cannot be empty"));
    }

    let description = body.description.unwrap_or_default();
    let tags = body.tags.unwrap_or_default();
    let now = now();
    let folder_path = slugify_string(name);

    match lima_db::queries::projects_create::create_project(
        state.db.pool(),
        name,
        &folder_path,
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
                return Err(ApiErrorResponse::new(StatusCode::CONFLICT, "existing_project", "Project with same name or path already exists").with_cause(&msg));
            } else {
                return Err(ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to create project").with_cause(&msg));
            }
        }
    }


}

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

fn slugify_string(input: &str) -> String {
    let out = input
        .trim()
        .to_lowercase()
        .replace(" ", "-")
        .replace("/", "-")
        .replace("\0", "-")
        .replace(".", "-")
        .replace("..", "-");
        

    out.trim_matches('-').to_string()
}