use axum::{extract::{Path, State}, http::StatusCode, Json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct PatchProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub main_image_id: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/projects/{project_id}",
    tag = "Projects",
    request_body = PatchProjectRequest,
    responses(
        (status = 200, description = "Project updated successfully"),
        (status = 400, description = "Bad Request", body = ApiErrorBody),
        (status = 404, description = "Project not found", body = ApiErrorBody),
        (status = 503, description = "Service Unavailable", body = ApiErrorBody),
    ),
    params(
        ("project_id" = String, Path, description = "The ID of the project to update"),
    )
)]
pub async fn project_update(
    State(app_state): State<AppState>,
    Path(project_id): Path<String>,
    Json(payload): Json<PatchProjectRequest>,
) -> Result<StatusCode, ApiErrorResponse> {
    if payload.name.is_none()
        && payload.description.is_none()
        && payload.main_image_id.is_none()
    {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "missing_fields", "At least one field must be provided for update."));
    }

    let now = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "time_format_error",
            "Failed to format current time: {}",
        ).with_cause(&e.to_string())
    })?;

    let updated_rows = lima_db::queries::projects_update::update_project(
        app_state.db.pool(),
        &project_id,
        payload.name.as_deref(),
        payload.description.as_deref(),
        payload.main_image_id.as_deref(),
        &now,
    )
    .await
    .map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "database_error",
            "Failed to update project",
        ).with_cause(&e.to_string())
    })?;
    
    // TODO: don't like this approach but for now will do.
    if updated_rows == 0 {
        return Err(ApiErrorResponse::new(
            StatusCode::NOT_FOUND,
            "no_update_done",
            "Nothing was updated, possibly because the project does not exist or same data was given.",
        ));
    };

    Ok(StatusCode::OK)
}
