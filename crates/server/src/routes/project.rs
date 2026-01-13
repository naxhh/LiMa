use axum::{
    extract::{State, Query},
    http::StatusCode,
    Json,
};
use utoipa::ToSchema;
use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::Cursor;

use crate::state::AppState;

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
        (status = 400, description = "Invalid parameter provided"),
        (status = 503, description = "Failure to connect to the database"),
    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    Query(params): Query<ListProjectsParams>,
) -> Result<Json<ListProjectsResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);

    let cursor = match params.cursor {
        Some(ref c) => Some(decode_cursor(c).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?),
        None => None,
    };

    // TODO: likely move this to its own method
    if let Some(query) = params.query.as_deref().filter(|query| !query.trim().is_empty()) {

        if let Some(ref c) = cursor {
            if c.rank.is_none() {
                return Err(StatusCode::BAD_REQUEST);
            }
        }

        let search_projects = lima_db::queries::projects_search::search_projects(
            state.db.pool(),
            query,
            limit,
            cursor,
        )
        .await
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;

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
    .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;

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


// TODO: move to wherever. domain maybe?
fn decode_cursor(cursor: &str) -> Result<Cursor, ()> {
    let bytes = general_purpose::STANDARD.decode(cursor).map_err(|_| ())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| ())?;
    
    Ok(cursor)
}

fn encode_cursor(cursor: &Cursor) -> String {
    general_purpose::STANDARD.encode(serde_json::to_vec(&cursor).unwrap())
}