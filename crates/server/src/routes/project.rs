use axum::{
    Json, extract::{Query, State}, http::StatusCode
};
use utoipa::ToSchema;
use serde::{Serialize, Deserialize};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::{Cursor, decode_cursor, encode_cursor};
use lima_db::queries::projects::ProjectSort;

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
    
    fn sort(&self) -> Result<(ProjectSort, bool), ApiErrorResponse> {
        parse_sort(self.sort.as_deref(), self.order.as_deref())
    }
}

fn invalid_sort(msg: &'static str) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_sort", msg)
}

/// Parses `order` for a sort key: asc/desc, or the key's default when missing
/// (A–Z for name, newest/largest first otherwise). Returns true for descending.
pub(crate) fn parse_order(order: Option<&str>, ascending_by_default: bool) -> Result<bool, ApiErrorResponse> {
    match order {
        None => Ok(!ascending_by_default),
        Some("asc") => Ok(false),
        Some("desc") => Ok(true),
        Some(_) => Err(invalid_sort("order must be asc or desc")),
    }
}

/// Sort key and direction (true = descending) for project lists.
pub(crate) fn parse_sort(sort: Option<&str>, order: Option<&str>) -> Result<(ProjectSort, bool), ApiErrorResponse> {
    let sort = match sort {
        None => ProjectSort::default(),
        Some(s) => ProjectSort::parse(s).ok_or_else(|| invalid_sort("sort must be one of: updated, created, modified, name, size, files"))?,
    };
    Ok((sort, parse_order(order, sort == ProjectSort::Name)?))
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
    /// Asset whose thumbnail stands in for the cover when there's no image (a model preview).
    pub preview_id: Option<String>,
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
                preview_id: s.preview_model_id,
            }
        })
        .collect())
}


/// Most ids returned by `/projects/ids` (the bulk action limit).
const MAX_IDS: i64 = 2000;

#[derive(Serialize, ToSchema)]
pub struct ProjectIdsResponse {
    pub ids: Vec<String>,
    /// More projects match than were returned.
    pub truncated: bool,
}

#[utoipa::path(
    get,
    path = "/api/projects/ids",
    tag = "Projects",
    params(
        ("query" = Option<String>, Query, description = "Search query, as for /api/projects"),
        ("tags" = Option<String>, Query, description = "Comma-separated tag ids; projects must have all of them"),
        ("formats" = Option<String>, Query, description = "Comma-separated model extensions; projects must have at least one"),
        ("no_image" = Option<bool>, Query, description = "Only projects without images"),
    ),
    responses((status = 200, description = "Ids of every project matching the search and filters (up to 2,000), for \"select all\"", body = ProjectIdsResponse))
)]
pub async fn list_project_ids(State(state): State<AppState>, Query(params): Query<ListProjectsParams>) -> Result<Json<ProjectIdsResponse>, ApiErrorResponse> {
    let filter = params.filter();
    let db = |e: sqlx::Error| ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed listing projects").with_cause(&e.to_string());
    let mut ids: Vec<String> = match params.query.as_deref().filter(|q| !q.trim().is_empty()) {
        Some(query) => lima_db::queries::projects_search::search_projects(state.db.pool(), query, MAX_IDS + 1, None, &filter)
            .await
            .map_err(db)?
            .into_iter()
            .map(|r| r.project.id)
            .collect(),
        None => lima_db::queries::projects::list_projects(state.db.pool(), MAX_IDS + 1, None, lima_db::queries::projects::ProjectSort::Updated, true, &filter)
            .await
            .map_err(db)?
            .into_iter()
            .map(|(p, _)| p.id)
            .collect(),
    };
    let truncated = ids.len() as i64 > MAX_IDS;
    ids.truncate(MAX_IDS as usize);
    Ok(Json(ProjectIdsResponse { ids, truncated }))
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
        ("sort" = Option<String>, Query, description = "Sort key: updated (default; last change seen by LiMa), created (added to LiMa), modified (newest file date on disk), name, size, files. Ignored when searching (results are by relevance)"),
        ("order" = Option<String>, Query, description = "Sort order: asc or desc (default: asc for name, desc otherwise). Ignored when searching"),
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

    let (sort, descending) = params.sort()?;
    let bad_cursor = |cause: &str| ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_cursor", "Invalid cursor parameter").with_cause(cause);
    // The cursor must come from a page with the same sort, or the next page would skip or repeat projects.
    let after = match cursor {
        None => None,
        Some(c) => match (c.sort.as_deref(), c.sort_value) {
            (Some(s), Some(value)) if s == sort.as_str() => Some((value, c.id)),
            _ => return Err(bad_cursor("Cursor was made for a different sort")),
        },
    };

    let projects_with_sort = lima_db::queries::projects::list_projects(
        state.db.pool(),
        limit,
        after,
        sort,
        descending,
        &filter,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::Protocol(ref m) if m == lima_db::queries::projects::INVALID_CURSOR => bad_cursor(m),
        e => ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "DB failed listing projects").with_cause(&e.to_string()),
    })?;

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
