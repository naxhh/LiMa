use axum::{
    Json, extract::{Query, State}, http::StatusCode
};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;

use lima_domain::pagination::{Cursor, decode_cursor, encode_cursor};
use crate::state::AppState;
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};


#[derive(Deserialize, ToSchema)]
pub struct ListTagsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct ListTagsResponse {
    pub items: Vec<Tag>,
    pub next_cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/tags",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of tags to return (default: 50, max: 200)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
    ),
    responses(
        (status = 200, description = "List of tags", body = ListTagsResponse),
        (status = 400, description = "Invalid parameter provided", body = ApiErrorBody),
        (status = 503, description = "Failure to connect to the database", body = ApiErrorBody),
    )
)]
pub async fn list_tags(
    State(state): State<AppState>,
    Query(params): Query<ListTagsParams>,
) -> Result<Json<ListTagsResponse>, ApiErrorResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);

    let cursor = match params.cursor {
        Some(ref c) => Some(decode_cursor(c).map_err(|e| ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Invalid cursor parameter").with_cause(&e))?),
        None => None,
    };

    let tags = lima_db::queries::tags::list_tags(
        state.db.pool(),
        limit,
        cursor,
    )
    .await
    .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed listing tags").with_cause(&e.to_string()))?;

    let next_cursor = tags.last().map(|last| {
        encode_cursor(&Cursor {
            updated_at: last.updated_at.clone(),
            id: last.id.clone(),
            rank: None
        })
    });    

    Ok(Json(ListTagsResponse {
        items: tags.into_iter().map(|row| Tag { id: row.id, name: row.name, color: row.color, created_at: row.created_at, updated_at: row.updated_at }).collect(),
        next_cursor,
    }))
}
