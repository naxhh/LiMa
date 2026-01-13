use axum::{
    extract::{State, Query},
    http::StatusCode,
    Json,
};
use utoipa::ToSchema;
use serde::{Serialize, Deserialize};
use lima_domain::models::project::ProjectRow;
use base64::{Engine as _, engine::general_purpose};

use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct ListProjectsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Cursor{
    updated_at: String,
    id: String,
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
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor")
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

    let projects = lima_db::queries::projects::list_projects(
        state.db.pool(),
        limit,
        cursor,
    )
    .await
    .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;

    let next_cursor = projects.last().map(|project| {
        encode_cursor(&project.updated_at, &project.id)
    });

    Ok(Json(ListProjectsResponse {
        items: projects,
        next_cursor,
    }))
}


// TODO: move to wherever. domain maybe?
fn decode_cursor(cursor: &str) -> Result<(String, String), ()> {
    let bytes =general_purpose::STANDARD.decode(cursor).map_err(|_| ())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| ())?;
    
    Ok((cursor.updated_at, cursor.id))
}

fn encode_cursor(updated_at: &str, id: &str) -> String {
    let cursor = Cursor {
        updated_at: updated_at.to_string(),
        id: id.to_string(),
    };

    general_purpose::STANDARD.encode(serde_json::to_vec(&cursor).unwrap())
}