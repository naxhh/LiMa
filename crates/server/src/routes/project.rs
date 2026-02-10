use axum::{
    Json, extract::{Query, State}, http::StatusCode
};
use utoipa::ToSchema;
use serde::{Serialize, Deserialize};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::{Cursor, decode_cursor, encode_cursor};

use crate::{models::http_error::ApiErrorBody, state::AppState};
use crate::models::http_error::ApiErrorResponse;

#[derive(Deserialize, ToSchema)]
pub struct ListProjectsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub query: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ListProjectsResponse {
    pub items: Vec<ProjectRow>,
    pub next_cursor: Option<String>,
}


#[utoipa::path(
    get,
    path = "/projects",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of projects to return (default: 50, max: 200)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("query" = Option<String>, Query, description = "Search query to filter projects"),
    ),
    responses(
        (status = 200, description = "List of projects", body = ListProjectsResponse),
        (status = 400, description = "Invalid parameter provided", body = ApiErrorBody),
        (status = 503, description = "Failure to connect to the database", body = ApiErrorBody),
    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    Query(params): Query<ListProjectsParams>,
) -> Result<Json<ListProjectsResponse>, ApiErrorResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);

    let cursor = match params.cursor {
        Some(ref c) => Some(decode_cursor(c).map_err(|e| ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Invalid cursor parameter").with_cause(&e))?),
        None => None,
    };

    // TODO: likely move this to its own method
    if let Some(query) = params.query.as_deref().filter(|query| !query.trim().is_empty()) {

        if let Some(ref c) = cursor {
            if c.rank.is_none() {
                return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Invalid cursor parameter").with_cause("Cursor is missing rank"));
            }
        }

        let search_projects = lima_db::queries::projects_search::search_projects(
            state.db.pool(),
            query,
            limit,
            cursor,
        )
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed searching projects").with_cause(&e.to_string()))?;

        let next_cursor = search_projects.last().map(|row| {
            encode_cursor(&Cursor {
                updated_at: row.project.updated_at.clone(),
                id: row.project.id.clone(),
                rank: Some(row.rank),
            })
        });

        let projects = search_projects.into_iter().map(|row| row.project).collect();

        return Ok(Json(ListProjectsResponse {
            items: projects,
            next_cursor,
        }));
    }


    let projects = lima_db::queries::projects::list_projects(
        state.db.pool(),
        limit,
        cursor,
    )
    .await
    .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed listing projects").with_cause(&e.to_string()))?;

    let next_cursor = projects.last().map(|project| {
        encode_cursor(&Cursor {
            updated_at: project.updated_at.clone(),
            id: project.id.clone(),
            rank: None,
        })
    });

    Ok(Json(ListProjectsResponse {
        items: projects,
        next_cursor,
    }))
}
