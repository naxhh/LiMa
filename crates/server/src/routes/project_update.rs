use axum::{extract::{Path, State}, http::StatusCode, Json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::state::AppState;
use crate::models::http_error::{ApiErrorResponse, ApiErrorBody};

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct PatchProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub main_image_id: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[utoipa::path(
    patch,
    path = "/api/projects/{project_id}",
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
        && payload.tags.is_none()
    {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "missing_fields", "At least one field must be provided for update."));
    }

    let mut tx = app_state.db.pool().begin().await.map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "database_error",
            "Failed to start database transaction",
        ).with_cause(&e.to_string())
    })?;

    let now = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|e| {
        ApiErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "time_format_error",
            "Failed to format current time: {}",
        ).with_cause(&e.to_string())
    })?;

    // Checked up front: an update that changes nothing still succeeds, a missing project doesn't.
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "database_error", "Failed to load project").with_cause(&e.to_string()))?;
    if exists.is_none() {
        return Err(ApiErrorResponse::new(StatusCode::NOT_FOUND, "project_not_found", "Project not found"));
    }

    if payload.name.is_some() || payload.description.is_some() || payload.main_image_id.is_some() {
        lima_db::queries::projects_update::update_project(
            &mut tx,
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
    }

    if let Some(raw_tags) = payload.tags {
        let tags = raw_tags
            .iter()
            .map(|t| lima_domain::models::tag::normalize_tag_name(t))
            .collect::<Option<Vec<String>>>()
            .ok_or_else(|| {
                ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_tag_name", "Tag names must be 1 to 64 characters")
            })?;

        let tag_ids = lima_db::queries::tags::ensure_tags(&mut tx, &tags, &now).await.map_err(|e| {
            ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                "Failed to ensure tags exist",
            ).with_cause(&e.to_string())
        })?;
        
        lima_db::queries::tags::set_project_tags(&mut tx, &project_id, &tag_ids).await.map_err(|e| {
            ApiErrorResponse::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_error",
                "Failed to set project tags",
            ).with_cause(&e.to_string())
        })?;
    }

    tx.commit().await.map_err(|e| {
        ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "database_error", "Failed to save project")
            .with_cause(&e.to_string())
    })?;

    Ok(StatusCode::OK)
}
