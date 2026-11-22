use axum::{
    Json, extract::{Query, State}, http::StatusCode
};
use utoipa::ToSchema;
use serde::{Serialize, Deserialize};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::{Cursor, decode_cursor, encode_cursor};

use crate::{models::http_error::ApiErrorBody, state::AppState};
use crate::models::http_error::ApiErrorResponse;
use crate::routes::project_detail::{ProjectCollectionResponse, ProjectTagResponse};

#[derive(Deserialize, ToSchema)]
pub struct ListProjectsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub query: Option<String>,
    /// Comma-separated tag ids; projects must have all of them.
    pub tags: Option<String>,
    /// Comma-separated model file extensions (`STL,3MF`); projects must have one of them.
    pub formats: Option<String>,
    /// Only projects without images.
    pub no_image: Option<bool>,
    /// Sort key for the results (updated, created, name, size, files).
    pub sort: Option<String>,
    /// Sort order (asc or desc).
    pub order: Option<String>,
}

impl ListProjectsParams {
    fn filter(&self) -> lima_db::queries::projects::ProjectFilter {
        let split = |v: &Option<String>| -> Vec<String> {
            v.as_deref()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        let mut formats: Vec<String> = split(&self.formats)
            .into_iter()
            .map(|f| f.trim_start_matches('.').to_ascii_uppercase())
            .filter(|f| !f.is_empty() && f.len() <= 10 && f.chars().all(|c| c.is_ascii_alphanumeric()))
            .collect();
        formats.dedup();
        let mut tag_ids = split(&self.tags);
        tag_ids.truncate(20);
        lima_db::queries::projects::ProjectFilter { tag_ids, formats, no_image: self.no_image.unwrap_or(false) }
    }
    
    fn sort(&self) -> lima_db::queries::projects::ProjectSort {
        match self.sort.as_deref() {
            Some("created") => lima_db::queries::projects::ProjectSort::Created,
            Some("name") => lima_db::queries::projects::ProjectSort::Name,
            Some("size") => lima_db::queries::projects::ProjectSort::Size,
            Some("files") => lima_db::queries::projects::ProjectSort::Files,
            _ => lima_db::queries::projects::ProjectSort::Updated, // default
        }
    }
    
    fn order(&self) -> bool {
        match self.order.as_deref() {
            Some("asc") => false,
            Some("desc") => true,
            _ => {
                // Default order depends on sort key
                match self.sort.as_deref() {
                    Some("name") => false,  // name ascending by default
                    _ => true,              // all others descending by default
                }
            }
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ListProjectsResponse {
    pub items: Vec<ProjectListItem>,
    pub next_cursor: Option<String>,
}

/// A project plus the aggregates list views need (counts, size, formats, tags).
#[derive(Serialize, ToSchema)]
pub struct ProjectListItem {
    #[serde(flatten)]
    pub project: ProjectRow,
    pub asset_count: i64,
    pub image_count: i64,
    pub model_count: i64,
    pub total_bytes: i64,
    /// Upper-case extensions of the model files, e.g. `["3MF", "STL"]`.
    pub model_formats: Vec<String>,
    pub tags: Vec<ProjectTagResponse>,
    /// The user's collections containing the project (Favourites is reported as `is_favourite`).
    pub collections: Vec<ProjectCollectionResponse>,
    pub is_favourite: bool,
}

pub(crate) async fn with_summaries(state: &AppState, projects: Vec<ProjectRow>) -> Result<Vec<ProjectListItem>, ApiErrorResponse> {
    let ids: Vec<String> = projects.iter().map(|p| p.id.clone()).collect();
    let mut summaries = lima_db::queries::projects_summary::project_summaries(state.db.pool(), &ids)
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed loading project summaries").with_cause(&e.to_string()))?;
    Ok(projects
        .into_iter()
        .map(|project| {
            let s = summaries.remove(&project.id).unwrap_or_default();
            ProjectListItem {
                project,
                asset_count: s.asset_count,
                image_count: s.image_count,
                model_count: s.model_count,
                total_bytes: s.total_bytes,
                model_formats: s.model_formats,
                tags: s.tags.into_iter().map(|t| ProjectTagResponse { id: t.id, name: t.name, color: t.color }).collect(),
                collections: s.collections.into_iter().map(|c| ProjectCollectionResponse { id: c.id, name: c.name }).collect(),
                is_favourite: s.is_favourite,
            }
        })
        .collect())
}


#[utoipa::path(
    get,
    path = "/api/projects",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of projects to return (default: 50, max: 200)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("query" = Option<String>, Query, description = "Search query to filter projects"),
        ("tags" = Option<String>, Query, description = "Comma-separated tag ids; projects must have all of them"),
        ("formats" = Option<String>, Query, description = "Comma-separated model extensions (STL,3MF,...); projects must have at least one"),
        ("no_image" = Option<bool>, Query, description = "Only projects without images"),
        ("sort" = Option<String>, Query, description = "Sort key for results: updated, created, name, size, files"),
        ("order" = Option<String>, Query, description = "Sort order: asc or desc"),
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
    let filter = params.filter();
    let sort = params.sort();
    let descending = params.order();

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
            &filter,
        )
        .await
        .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed searching projects").with_cause(&e.to_string()))?;

        let next_cursor = search_projects.last().map(|row| {
            encode_cursor(&Cursor {
                updated_at: row.project.updated_at.clone(),
                id: row.project.id.clone(),
                rank: Some(row.rank),
                sort: None,
                sort_value: None,
            })
        });

        let projects = with_summaries(&state, search_projects.into_iter().map(|row| row.project).collect()).await?;

        return Ok(Json(ListProjectsResponse {
            items: projects,
            next_cursor,
        }));
    }

    // For non-search requests, use the new sorting and pagination
    let cursor_value = match cursor {
        Some(c) => {
            // Validate that cursor sort matches current sort
            if let Some(cursor_sort) = &c.sort {
                if let Some(current_sort) = lima_db::queries::projects::ProjectSort::parse(cursor_sort) {
                    if current_sort != sort {
                        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Cursor sort does not match requested sort"));
                    }
                }
            }
            
            // For size and files sorts, we need to bind as i64
            let sort_value = c.sort_value.map(|v| (v, c.id));
            sort_value
        },
        None => None,
    };

    let projects_with_sort = lima_db::queries::projects::list_projects(
        state.db.pool(),
        limit,
        cursor_value,
        sort,
        descending,
        &filter,
    )
    .await
    .map_err(|e| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed listing projects").with_cause(&e.to_string()))?;

    let next_cursor = projects_with_sort.last().map(|(project, sort_value)| {
        encode_cursor(&Cursor {
            updated_at: project.updated_at.clone(),
            id: project.id.clone(),
            rank: None,
            sort: Some(sort.as_str().to_string()),
            sort_value: Some(sort_value.clone()),
        })
    });

    let projects = projects_with_sort.into_iter().map(|(project, _)| project).collect();
    
    Ok(Json(ListProjectsResponse {
        items: with_summaries(&state, projects).await?,
        next_cursor,
    }))
}
