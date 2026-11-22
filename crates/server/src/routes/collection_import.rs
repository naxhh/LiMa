//! Importing a whole collection from Printables, MakerWorld or Thingiverse, and its sync settings.

use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use lima_db::queries::collection_sources::{self as db, SourceRow};
use lima_importers::collections::{CollectionRef, parse_collection_url};
use lima_importers::{ImportError, Source, Tokens};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::collection_sync::{self, BAMBU_TOKEN, CollectionJob, source_name};
use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::bambu;
use crate::routes::imports::{fetch_error, now_rfc3339};
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub struct CollectionPreviewRequest {
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionPreviewResponse {
    /// `printables`, `makerworld` or `thingiverse`.
    pub source: String,
    pub url: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub models: Vec<CollectionPreviewModel>,
    /// The collection has more models than can be imported at once (1,000).
    pub truncated: bool,
    /// Already imported: the LiMa collection it went to.
    pub collection_id: Option<String>,
    /// Downloading files needs a Bambu login (MakerWorld).
    pub login_required: bool,
    pub logged_in: bool,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionPreviewModel {
    pub id: String,
    pub name: String,
    pub url: String,
    pub cover: Option<String>,
    pub author: Option<String>,
    /// The library already has a project from this model page: it's linked, not downloaded.
    pub project_id: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct StartCollectionImportRequest {
    pub url: String,
    /// Models to import, from the preview. The rest are left out (also by later syncs).
    pub model_ids: Vec<String>,
    /// Keep importing models added to the source collection, and delete (to the trash) the
    /// ones removed from it. MakerWorld stores the Bambu login on the server for this.
    #[serde(default)]
    pub keep_in_sync: bool,
    /// LiMa collection name; defaults to the source collection's.
    pub name: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct StartCollectionImportResponse {
    pub job_id: String,
    pub collection_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionSourceResponse {
    pub source: String,
    pub url: String,
    pub keep_in_sync: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    /// Models seen in the source collection.
    pub models: usize,
    /// Models left out (unticked, or whose project was deleted).
    pub excluded: usize,
    /// Models that couldn't be imported yet (retried on the next sync).
    pub failed: usize,
    /// The sync running now, if any.
    pub job_id: Option<String>,
    /// MakerWorld: whether the server has a Bambu login for background syncs.
    pub has_login: bool,
    /// How often synced collections are checked, in hours (0 = only by hand).
    pub interval_hours: u64,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateCollectionSourceRequest {
    pub keep_in_sync: bool,
}

#[derive(Serialize, ToSchema)]
pub struct StartSyncResponse {
    pub job_id: String,
}

fn db_error(e: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Database failure").with_cause(&e.to_string())
}

fn parse(url: &str) -> Result<CollectionRef, ApiErrorResponse> {
    parse_collection_url(url).ok_or_else(|| {
        ApiErrorResponse::new(StatusCode::BAD_REQUEST, "unsupported_url", "Only Printables, MakerWorld and Thingiverse collection links can be imported.")
    })
}

async fn fetch(state: &AppState, c: &CollectionRef) -> Result<lima_importers::collections::RemoteCollection, ApiErrorResponse> {
    let tokens = Tokens { thingiverse: state.thingiverse_token.as_deref(), ..Default::default() };
    lima_importers::collections::fetch_collection(&state.http, c, tokens).await.map_err(|e| match e {
        ImportError::NotFound => ApiErrorResponse::new(StatusCode::NOT_FOUND, "collection_not_found", "No collection found at that link."),
        e => fetch_error(e),
    })
}

/// The Bambu token from this browser, checked with Bambu. `Err` asks the UI for a login.
async fn bambu_token(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    let token = bambu::token_from_headers(headers).ok_or_else(bambu::login_required)?;
    match lima_importers::bambu::verify_token(&state.http, &token).await {
        Ok(false) => Err(bambu::login_required()),
        Ok(true) => Ok(token),
        Err(e) => {
            tracing::warn!("Could not check the Bambu token: {e}");
            Ok(token)
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/imports/collections/preview",
    tag = "Imports",
    request_body = CollectionPreviewRequest,
    responses(
        (status = 200, description = "The collection and its models", body = CollectionPreviewResponse),
        (status = 400, description = "Not a Printables, MakerWorld or Thingiverse collection link", body = ApiErrorBody),
        (status = 404, description = "Collection not found", body = ApiErrorBody),
        (status = 412, description = "Thingiverse token missing or rejected", body = ApiErrorBody),
    )
)]
pub async fn collection_preview(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<CollectionPreviewRequest>) -> Result<Json<CollectionPreviewResponse>, ApiErrorResponse> {
    let c = parse(&body.url)?;
    let remote = fetch(&state, &c).await?;
    let pool = state.db.pool();
    let mut models = Vec::with_capacity(remote.models.len());
    for m in remote.models {
        let project_id = db::project_for_url(pool, &m.url).await.map_err(db_error)?;
        models.push(CollectionPreviewModel { id: m.id, name: m.name, url: m.url, cover: m.cover, author: m.author, project_id });
    }
    let linked = db::by_source(pool, source_name(remote.source), &remote.source_id).await.map_err(db_error)?;
    Ok(Json(CollectionPreviewResponse {
        source: source_name(remote.source).into(),
        url: remote.url,
        name: remote.name,
        description: remote.description,
        author: remote.author,
        models,
        truncated: remote.truncated,
        collection_id: linked.map(|l| l.collection_id),
        login_required: remote.source.downloads_need_login(),
        logged_in: bambu::token_from_headers(&headers).is_some(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/imports/collections",
    tag = "Imports",
    request_body = StartCollectionImportRequest,
    responses(
        (status = 202, description = "LiMa collection created; models are importing", body = StartCollectionImportResponse),
        (status = 401, description = "Bambu login required (code bambu_login_required)", body = ApiErrorBody),
        (status = 409, description = "Already imported (code collection_already_imported)", body = ApiErrorBody),
    )
)]
pub async fn start_collection_import(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<StartCollectionImportRequest>) -> Response {
    match start_inner(state, headers, body).await {
        Ok(r) => (StatusCode::ACCEPTED, Json(r)).into_response(),
        Err(r) => r,
    }
}

async fn start_inner(state: AppState, headers: HeaderMap, body: StartCollectionImportRequest) -> Result<StartCollectionImportResponse, Response> {
    let c = parse(&body.url).map_err(IntoResponse::into_response)?;
    let remote = fetch(&state, &c).await.map_err(IntoResponse::into_response)?;
    let pool = state.db.pool();
    let source = source_name(remote.source);
    if let Some(existing) = db::by_source(pool, source, &remote.source_id).await.map_err(|e| db_error(e).into_response())? {
        return Err(ApiErrorResponse::new(StatusCode::CONFLICT, "collection_already_imported", "That collection was already imported.")
            .with_cause(&existing.collection_id)
            .into_response());
    }

    let selected: HashSet<String> = body.model_ids.into_iter().collect();
    let token = if remote.source == Source::Makerworld && !selected.is_empty() { Some(bambu_token(&state, &headers).await?) } else { None };
    let now = now_rfc3339();
    if body.keep_in_sync {
        if let Some(t) = &token {
            db::set_token(pool, BAMBU_TOKEN, t, &now).await.map_err(|e| db_error(e).into_response())?;
        }
    }

    // A free collection name: "Toys", "Toys (2)", ...
    let base = body.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&remote.name).to_string();
    let base = if base.is_empty() { format!("{source} collection {}", remote.source_id) } else { base };
    let mut collection = None;
    for n in 1..=50 {
        let name = if n == 1 { base.clone() } else { format!("{base} ({n})") };
        if lima_db::queries::collections::find_collection_id_by_name(pool, &name).await.map_err(|e| db_error(e).into_response())?.is_some() {
            continue;
        }
        collection = Some(lima_db::queries::collections::create_collection(pool, &name, &remote.description, &now).await.map_err(|e| db_error(e).into_response())?);
        break;
    }
    let collection = collection.ok_or_else(|| ApiErrorResponse::new(StatusCode::CONFLICT, "collection_name_taken", "No free collection name").into_response())?;

    let row = SourceRow {
        id: uuid::Uuid::new_v4().to_string(),
        collection_id: collection.id.clone(),
        source: source.into(),
        source_id: remote.source_id.clone(),
        url: remote.url.clone(),
        keep_in_sync: body.keep_in_sync,
        last_synced_at: None,
        last_error: None,
        created_at: now,
    };
    db::create(pool, &row).await.map_err(|e| db_error(e).into_response())?;
    let job_id = collection_sync::start(&state, row, Some(selected), token, "import").await;
    Ok(StartCollectionImportResponse { job_id, collection_id: collection.id })
}

#[utoipa::path(
    get,
    path = "/api/imports/collections/{job_id}",
    tag = "Imports",
    params(("job_id" = String, Path)),
    responses(
        (status = 200, description = "Progress of a collection import or sync", body = CollectionJob),
        (status = 404, description = "Unknown job (or finished more than an hour ago)", body = ApiErrorBody),
    )
)]
pub async fn collection_job_status(State(state): State<AppState>, Path(job_id): Path<String>) -> Result<Json<CollectionJob>, ApiErrorResponse> {
    collection_sync::job(&state, &job_id)
        .map(Json)
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "job_not_found", "Unknown import job"))
}

async fn source_of(state: &AppState, collection_id: &str) -> Result<SourceRow, ApiErrorResponse> {
    db::by_collection(state.db.pool(), collection_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "no_source", "This collection wasn't imported from a site"))
}

async fn source_response(state: &AppState, s: SourceRow) -> Result<CollectionSourceResponse, ApiErrorResponse> {
    let items = db::items(state.db.pool(), &s.id).await.map_err(db_error)?;
    let has_login = s.source != "makerworld" || db::token(state.db.pool(), BAMBU_TOKEN).await.map_err(db_error)?.is_some();
    Ok(CollectionSourceResponse {
        job_id: collection_sync::running_job(state, &s.collection_id),
        models: items.len(),
        excluded: items.iter().filter(|i| i.excluded).count(),
        failed: items.iter().filter(|i| !i.excluded && i.project_id.is_none() && i.last_error.is_some()).count(),
        source: s.source,
        url: s.url,
        keep_in_sync: s.keep_in_sync,
        last_synced_at: s.last_synced_at,
        last_error: s.last_error,
        has_login,
        interval_hours: state.collection_sync_hours,
    })
}

#[utoipa::path(
    get,
    path = "/api/collections/{collection_id}/source",
    tag = "Collections",
    params(("collection_id" = String, Path)),
    responses(
        (status = 200, description = "Where the collection was imported from, and its sync state", body = CollectionSourceResponse),
        (status = 404, description = "Not imported from a site", body = ApiErrorBody),
    )
)]
pub async fn get_collection_source(State(state): State<AppState>, Path(collection_id): Path<String>) -> Result<Json<CollectionSourceResponse>, ApiErrorResponse> {
    let s = source_of(&state, &collection_id).await?;
    Ok(Json(source_response(&state, s).await?))
}

#[utoipa::path(
    patch,
    path = "/api/collections/{collection_id}/source",
    tag = "Collections",
    params(("collection_id" = String, Path)),
    request_body = UpdateCollectionSourceRequest,
    responses(
        (status = 200, description = "Updated", body = CollectionSourceResponse),
        (status = 401, description = "Turning on sync for MakerWorld needs a Bambu login", body = ApiErrorBody),
        (status = 404, description = "Not imported from a site", body = ApiErrorBody),
    )
)]
pub async fn update_collection_source(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateCollectionSourceRequest>,
) -> Result<Json<CollectionSourceResponse>, Response> {
    let s = source_of(&state, &collection_id).await.map_err(IntoResponse::into_response)?;
    if body.keep_in_sync && s.source == "makerworld" {
        let token = bambu_token(&state, &headers).await?;
        db::set_token(state.db.pool(), BAMBU_TOKEN, &token, &now_rfc3339()).await.map_err(|e| db_error(e).into_response())?;
    }
    db::set_keep_in_sync(state.db.pool(), &s.id, body.keep_in_sync).await.map_err(|e| db_error(e).into_response())?;
    let s = source_of(&state, &collection_id).await.map_err(IntoResponse::into_response)?;
    Ok(Json(source_response(&state, s).await.map_err(IntoResponse::into_response)?))
}

#[utoipa::path(
    post,
    path = "/api/collections/{collection_id}/source/sync",
    tag = "Collections",
    params(("collection_id" = String, Path)),
    responses(
        (status = 202, description = "Sync started (or already running)", body = StartSyncResponse),
        (status = 404, description = "Not imported from a site", body = ApiErrorBody),
    )
)]
pub async fn sync_collection_now(State(state): State<AppState>, Path(collection_id): Path<String>, headers: HeaderMap) -> Result<(StatusCode, Json<StartSyncResponse>), ApiErrorResponse> {
    let s = source_of(&state, &collection_id).await?;
    // This browser's login first (and refresh the stored one with it), else the stored one.
    let token = if s.source == "makerworld" {
        match bambu::token_from_headers(&headers) {
            Some(t) => {
                if s.keep_in_sync {
                    db::set_token(state.db.pool(), BAMBU_TOKEN, &t, &now_rfc3339()).await.map_err(db_error)?;
                }
                Some(t)
            }
            None => db::token(state.db.pool(), BAMBU_TOKEN).await.map_err(db_error)?,
        }
    } else {
        None
    };
    let job_id = collection_sync::start(&state, s, None, token, "manual").await;
    Ok((StatusCode::ACCEPTED, Json(StartSyncResponse { job_id })))
}
