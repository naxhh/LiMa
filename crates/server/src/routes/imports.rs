//! Import a model from a Printables or MakerWorld URL.
//!
//! `POST /imports/preview` shows what the page holds without downloading anything.
//! `POST /imports` creates the project right away (so name conflicts surface
//! immediately), then downloads the chosen files in the background into a bundle
//! and imports it like an upload. `GET /imports/{id}` reports progress.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use lima_domain::models::bundle::FileMeta;
use lima_importers::{ImportError, RemoteFile, RemoteImage, RemoteModel};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::routes::bambu;
use crate::state::AppState;

/// Finished jobs are forgotten after this long.
const KEEP_FINISHED_SECS: i64 = 3600;

pub type ImportJobs = Arc<Mutex<HashMap<String, ImportJobResponse>>>;

#[derive(Deserialize, ToSchema)]
pub struct ImportPreviewRequest {
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct ImportPreviewResponse {
    /// `printables` or `makerworld`.
    pub source: String,
    pub source_id: String,
    pub url: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub tags: Vec<String>,
    pub images: Vec<ImportImage>,
    pub files: Vec<ImportFile>,
    /// Downloading files from this source needs a Bambu login.
    pub login_required: bool,
    /// This browser has a Bambu token cookie (not checked with Bambu yet).
    pub logged_in: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ImportImage {
    pub url: String,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ImportFile {
    pub id: String,
    pub name: String,
    pub size: Option<i64>,
    /// `model`, `gcode` or `other`.
    pub kind: String,
    pub group: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct StartImportRequest {
    pub url: String,
    /// Project name; defaults to the model name.
    pub name: Option<String>,
    /// Defaults to the model description.
    pub description: Option<String>,
    /// Defaults to the model tags.
    pub tags: Option<Vec<String>>,
    /// Ids from the preview's `files` to download.
    #[serde(default)]
    pub file_ids: Vec<String>,
    /// Download the model images too (default true).
    pub include_images: Option<bool>,
    /// Extract downloaded zip files, keeping their folders (default false).
    #[serde(default)]
    pub extract_zips: bool,
    /// With `extract_zips`, also keep the original archives (default false).
    #[serde(default)]
    pub keep_zips: bool,
}

#[derive(Serialize, ToSchema)]
pub struct StartImportResponse {
    pub job_id: String,
    pub project_id: String,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ImportJobResponse {
    pub id: String,
    pub project_id: String,
    /// `downloading`, `importing`, `complete` or `failed`. A failed import removes its project.
    pub status: String,
    pub total: usize,
    pub done: usize,
    /// File being downloaded.
    pub current: Option<String>,
    /// Files that could not be downloaded (the rest were imported).
    pub warnings: Vec<String>,
    pub error: Option<String>,
    /// e.g. `bambu_login_required` when the Bambu token stopped working mid-import.
    pub error_code: Option<String>,
    #[serde(skip)]
    finished_at: Option<i64>,
}

pub(crate) fn fetch_error(e: ImportError) -> ApiErrorResponse {
    match e {
        ImportError::UnsupportedUrl => ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "unsupported_url",
            "Only Printables, MakerWorld and Thingiverse model links are supported.",
        ),
        ImportError::TokenMissing => ApiErrorResponse::new(
            StatusCode::PRECONDITION_FAILED,
            "thingiverse_token_missing",
            "Thingiverse imports need an app token. Create one at thingiverse.com/apps/create and set LIMA_THINGIVERSE_TOKEN on the server.",
        ),
        ImportError::TokenRejected => ApiErrorResponse::new(
            StatusCode::PRECONDITION_FAILED,
            "thingiverse_token_rejected",
            "Thingiverse rejected the app token in LIMA_THINGIVERSE_TOKEN.",
        ),
        ImportError::NotFound => ApiErrorResponse::new(StatusCode::NOT_FOUND, "model_not_found", "No model found at that link."),
        e => ApiErrorResponse::new(StatusCode::BAD_GATEWAY, "upstream_error", "Could not read the model page.")
            .with_cause(&e.to_string()),
    }
}

async fn fetch(state: &AppState, url: &str) -> Result<RemoteModel, ApiErrorResponse> {
    let Some(model_ref) = lima_importers::parse_url(url) else {
        if lima_importers::collections::parse_collection_url(url).is_some() {
            return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "collection_url", "That's a collection link: use the collection import."));
        }
        return Err(fetch_error(ImportError::UnsupportedUrl));
    };
    let tokens = lima_importers::Tokens { thingiverse: state.thingiverse_token.as_deref(), ..Default::default() };
    lima_importers::fetch_model(&state.http, &model_ref, tokens).await.map_err(fetch_error)
}

#[utoipa::path(
    post,
    path = "/api/imports/preview",
    request_body = ImportPreviewRequest,
    responses(
        (status = 200, description = "What the model page holds", body = ImportPreviewResponse),
        (status = 400, description = "Unsupported URL", body = ApiErrorBody),
        (status = 404, description = "Model not found", body = ApiErrorBody),
        (status = 502, description = "The site answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn import_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ImportPreviewRequest>,
) -> Result<Json<ImportPreviewResponse>, ApiErrorResponse> {
    let m = fetch(&state, &body.url).await?;
    Ok(Json(ImportPreviewResponse {
        source: source_name(&m).into(),
        source_id: m.source_id,
        url: m.url,
        name: m.name,
        description: m.description,
        author: m.author,
        license: m.license,
        tags: m.tags,
        images: m.images.into_iter().map(|i| ImportImage { url: i.url, name: i.name }).collect(),
        files: m
            .files
            .into_iter()
            .map(|f| ImportFile { id: f.id, name: f.name, size: f.size, kind: f.kind, group: f.group })
            .collect(),
        login_required: m.source.downloads_need_login(),
        logged_in: bambu::token_from_headers(&headers).is_some(),
    }))
}

pub(crate) fn source_name(m: &RemoteModel) -> &'static str {
    match m.source {
        lima_importers::Source::Printables => "printables",
        lima_importers::Source::Makerworld => "makerworld",
        lima_importers::Source::Thingiverse => "thingiverse",
    }
}

#[utoipa::path(
    post,
    path = "/api/imports",
    request_body = StartImportRequest,
    responses(
        (status = 202, description = "Project created; files are downloading", body = StartImportResponse),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Bambu login required (code bambu_login_required)", body = ApiErrorBody),
        (status = 404, description = "Model not found", body = ApiErrorBody),
        (status = 409, description = "A project with that name already exists", body = ApiErrorBody),
        (status = 502, description = "The site answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn start_import(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<StartImportRequest>) -> Response {
    match start_inner(state, headers, body).await {
        Ok(res) => (StatusCode::ACCEPTED, Json(res)).into_response(),
        Err(res) => res,
    }
}

// reason: axum's Response error type is fixed; boxing it would change every call site.
#[allow(clippy::result_large_err)]
async fn start_inner(state: AppState, headers: HeaderMap, body: StartImportRequest) -> Result<StartImportResponse, Response> {
    let model = fetch(&state, &body.url).await.map_err(IntoResponse::into_response)?;

    let mut files: Vec<RemoteFile> = Vec::with_capacity(body.file_ids.len());
    for id in &body.file_ids {
        match model.files.iter().find(|f| &f.id == id) {
            Some(f) if !files.iter().any(|x| x.id == f.id) => files.push(f.clone()),
            Some(_) => {}
            None => {
                return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "unknown_file", "A selected file is no longer on the model page.")
                    .with_cause(id)
                    .into_response());
            }
        }
    }
    let images = if body.include_images.unwrap_or(true) { model.images.clone() } else { vec![] };

    // Check the token before creating anything, so the UI can ask for a login and retry.
    let token = if model.source.downloads_need_login() && !files.is_empty() {
        let Some(token) = bambu::token_from_headers(&headers) else {
            return Err(bambu::login_required());
        };
        match lima_importers::bambu::verify_token(&state.http, &token).await {
            Ok(true) => Some(token),
            Ok(false) => return Err(bambu::login_required()),
            // Bambu unreachable for the check; let the download itself decide.
            Err(e) => {
                tracing::warn!("Could not check Bambu token before import: {e}");
                Some(token)
            }
        }
    } else {
        None
    };

    let name = body.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&model.name).to_string();
    if name.is_empty() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "empty_name", "Project name cannot be empty").into_response());
    }
    let description = body.description.clone().unwrap_or_else(|| model.description.clone());
    let tags: Vec<String> = body
        .tags
        .clone()
        .unwrap_or_else(|| model.tags.clone())
        .iter()
        .filter_map(|t| lima_domain::models::tag::normalize_tag_name(t))
        .collect();

    let now = now_rfc3339();
    let folder_path = crate::routes::project_create::slugify_string(&name);
    if folder_path.is_empty() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "empty_name", "Project name cannot be empty").into_response());
    }
    let project = lima_db::queries::projects_create::create_project(
        state.db.pool(),
        &state.paths,
        &name,
        &folder_path,
        &description,
        None,
        &tags,
        &now,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.to_ascii_lowercase().contains("unique") {
            ApiErrorResponse::new(StatusCode::CONFLICT, "existing_project", "A project with that name already exists.")
                .with_cause(&msg)
                .into_response()
        } else {
            ApiErrorResponse::new(StatusCode::SERVICE_UNAVAILABLE, "db_failure", "Failed to create project").with_cause(&msg).into_response()
        }
    })?;

    if let Err(e) = lima_db::queries::projects_source::set_project_source(
        state.db.pool(),
        &project.id,
        &model.url,
        model.author.as_deref(),
        model.license.as_deref(),
    )
    .await
    {
        tracing::warn!("Could not record the source of project {}: {e}", project.id);
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let job = ImportJobResponse {
        id: job_id.clone(),
        project_id: project.id.clone(),
        status: "downloading".into(),
        total: images.len() + files.len(),
        done: 0,
        current: None,
        warnings: vec![],
        error: None,
        error_code: None,
        finished_at: None,
    };
    {
        let mut jobs = state.imports.lock().unwrap();
        let cutoff = OffsetDateTime::now_utc().unix_timestamp() - KEEP_FINISHED_SECS;
        jobs.retain(|_, j| j.finished_at.is_none_or(|t| t > cutoff));
        jobs.insert(job_id.clone(), job);
    }

    tracing::info!("Importing {} into project {} ({} images, {} files)", model.url, project.id, images.len(), files.len());
    let task = ImportTask { state: state.clone(), job_id: job_id.clone(), project_id: project.id.clone(), model, images, files, token, extract_zips: body.extract_zips, keep_zips: body.keep_zips };
    tokio::spawn(task.run());

    Ok(StartImportResponse { job_id, project_id: project.id })
}

#[utoipa::path(
    get,
    path = "/api/imports/{job_id}",
    params(("job_id" = String, Path, description = "Import job id")),
    responses(
        (status = 200, description = "Import progress", body = ImportJobResponse),
        (status = 404, description = "Unknown job (or finished more than an hour ago)", body = ApiErrorBody),
    )
)]
pub async fn import_status(State(state): State<AppState>, UrlPath(job_id): UrlPath<String>) -> Result<Json<ImportJobResponse>, ApiErrorResponse> {
    state
        .imports
        .lock()
        .unwrap()
        .get(&job_id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiErrorResponse::new(StatusCode::NOT_FOUND, "job_not_found", "Import job not found."))
}

pub(crate) fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default()
}

struct ImportTask {
    state: AppState,
    job_id: String,
    project_id: String,
    model: RemoteModel,
    images: Vec<RemoteImage>,
    files: Vec<RemoteFile>,
    token: Option<String>,
    extract_zips: bool,
    keep_zips: bool,
}

/// Imports one model into a new project right away (no job): creates the project under a
/// free name, downloads `files` and the images, and imports them. Used by collection
/// imports. On failure nothing is left behind. Returns the project id.
pub(crate) async fn import_model_now(
    state: &AppState,
    model: RemoteModel,
    files: Vec<RemoteFile>,
    bambu_token: Option<String>,
) -> Result<String, ModelImportError> {
    let now = now_rfc3339();
    let tags: Vec<String> = model.tags.iter().filter_map(|t| lima_domain::models::tag::normalize_tag_name(t)).collect();
    let base = if model.name.trim().is_empty() { format!("{} {}", source_name(&model), model.source_id) } else { model.name.clone() };
    let mut project = None;
    for n in 1..=50 {
        let name = if n == 1 { base.clone() } else { format!("{base} ({n})") };
        let folder = crate::routes::project_create::slugify_string(&name);
        if folder.is_empty() || state.paths.project_dir(&folder).exists() {
            continue;
        }
        match lima_db::queries::projects_create::create_project(state.db.pool(), &state.paths, &name, &folder, &model.description, None, &tags, &now).await {
            Ok(p) => {
                project = Some(p);
                break;
            }
            Err(e) if e.to_string().to_ascii_lowercase().contains("unique") => continue,
            Err(e) => return Err(ModelImportError::Other(format!("Could not create the project: {e}"))),
        }
    }
    let project = project.ok_or_else(|| ModelImportError::Other("No free project name".into()))?;
    if let Err(e) = lima_db::queries::projects_source::set_project_source(state.db.pool(), &project.id, &model.url, model.author.as_deref(), model.license.as_deref()).await {
        tracing::warn!("Could not record the source of project {}: {e}", project.id);
    }

    let task = ImportTask {
        state: state.clone(),
        job_id: String::new(),
        project_id: project.id.clone(),
        images: model.images.clone(),
        model,
        files,
        token: bambu_token,
        extract_zips: false,
        keep_zips: false,
    };
    let bundle_id = uuid::Uuid::new_v4().to_string();
    let bundle_dir = state.paths.bundle(&bundle_id).expect("generated bundle ids are UUIDs");
    let result = task.download_and_import(&bundle_id, &bundle_dir).await;
    let _ = tokio::fs::remove_dir_all(&bundle_dir).await;
    match result {
        Ok(()) => Ok(project.id),
        Err(fatal) => {
            if let Err(e) = lima_db::queries::projects_delete::delete_project(state.db.pool(), &state.paths, &project.id).await {
                tracing::error!("Could not remove project {} after a failed import: {e:?}", project.id);
            }
            Err(if fatal.code == "bambu_login_required" { ModelImportError::LoginRequired } else { ModelImportError::Other(fatal.message) })
        }
    }
}

#[derive(Debug)]
pub(crate) enum ModelImportError {
    /// The Bambu token was refused: stop, every other MakerWorld download will fail too.
    LoginRequired,
    Other(String),
}

struct Fatal {
    code: &'static str,
    message: String,
}

impl ImportTask {
    fn update(&self, f: impl FnOnce(&mut ImportJobResponse)) {
        if let Some(job) = self.state.imports.lock().unwrap().get_mut(&self.job_id) {
            f(job);
        }
    }

    async fn run(self) {
        let bundle_id = uuid::Uuid::new_v4().to_string();
        let bundle_dir = self.state.paths.bundle(&bundle_id).expect("generated bundle ids are UUIDs");

        let result = self.download_and_import(&bundle_id, &bundle_dir).await;
        let _ = tokio::fs::remove_dir_all(&bundle_dir).await;

        match result {
            Ok(()) => {
                tracing::info!("Import job {} complete", self.job_id);
                self.update(|j| {
                    j.status = "complete".into();
                    j.current = None;
                    j.finished_at = Some(OffsetDateTime::now_utc().unix_timestamp());
                });
            }
            Err(fatal) => {
                tracing::warn!("Import job {} failed: {}", self.job_id, fatal.message);
                // Nothing useful was imported: remove the project so a retry starts clean.
                if let Err(e) = lima_db::queries::projects_delete::delete_project(self.state.db.pool(), &self.state.paths, &self.project_id).await {
                    tracing::error!("Could not remove project {} after failed import: {e:?}", self.project_id);
                }
                self.update(|j| {
                    j.status = "failed".into();
                    j.current = None;
                    j.error = Some(fatal.message);
                    j.error_code = Some(fatal.code.into());
                    j.finished_at = Some(OffsetDateTime::now_utc().unix_timestamp());
                });
            }
        }
    }

    async fn download_and_import(&self, bundle_id: &str, dir: &Path) -> Result<(), Fatal> {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| Fatal { code: "filesystem_error", message: format!("Could not create the download folder: {e}") })?;

        let http = &self.state.http;
        let mut metas: Vec<FileMeta> = Vec::new();
        let mut files_ok = 0;

        for img in &self.images {
            self.update(|j| j.current = Some(img.name.clone()));
            let res = lima_importers::download_image(http, &self.model, img, dir).await;
            self.collect(&img.name, res, &mut metas).await?;
        }
        for file in &self.files {
            self.update(|j| j.current = Some(file.name.clone()));
            let tokens = lima_importers::Tokens { bambu: self.token.as_deref(), thingiverse: self.state.thingiverse_token.as_deref() };
            let res = lima_importers::download_file(http, &self.model, file, tokens, dir).await;
            if self.collect(&file.name, res, &mut metas).await? {
                files_ok += 1;
            }
        }

        if !self.files.is_empty() && files_ok == 0 {
            return Err(Fatal { code: "downloads_failed", message: "None of the selected files could be downloaded.".into() });
        }
        self.update(|j| {
            j.status = "importing".into();
            j.current = None;
        });
        if metas.is_empty() {
            return Ok(());
        }

        crate::bundles::write_bundle_meta(dir, metas)
            .await
            .map_err(|e| Fatal { code: "filesystem_error", message: format!("Could not write the bundle: {e}") })?;

        if self.extract_zips {
            // A zip that can't be extracted is imported as it is.
            match crate::archive::extract_bundle_zips(dir, &[], self.keep_zips).await {
                Ok(summary) => {
                    if !summary.skipped.is_empty() {
                        let n = summary.skipped.len();
                        self.update(|j| j.warnings.push(format!("{n} file(s) in zips were left out (password protected or unsupported)")));
                    }
                }
                Err(e) => {
                    let msg = match e {
                        crate::archive::ExtractError::Archive { name, error } => format!("{name} wasn't extracted: {error}"),
                        crate::archive::ExtractError::Io(e) => format!("Zips weren't extracted: {e}"),
                    };
                    self.update(|j| j.warnings.push(msg));
                }
            }
        }

        let assets = lima_db::queries::projects_import::import_assets_from_bundle(self.state.db.pool(), &self.state.paths, &self.project_id, bundle_id)
            .await
            .map_err(|e| Fatal { code: "import_failed", message: e.to_string() })?;

        if let Ok(project) = lima_db::queries::projects_detail::get_project(self.state.db.pool(), &self.project_id).await {
            crate::routes::project_import::generate_thumbnails(&self.state, &self.project_id, &project.folder_path, &assets).await;
        }

        // Images were downloaded first, in the site's order: the first is the cover.
        if let Some(cover) = assets.iter().find(|a| a.kind == "image")
            && let Err(e) = lima_db::queries::projects_update::set_main_image(self.state.db.pool(), &self.project_id, &cover.id, &now_rfc3339()).await
        {
            tracing::warn!("Could not set the cover of project {}: {e}", self.project_id);
        }
        Ok(())
    }

    /// Records one download. `Ok(true)` when it succeeded, `Ok(false)` when it
    /// failed but the import can go on; `Err` when the whole import must stop.
    async fn collect(&self, name: &str, res: lima_importers::Result<std::path::PathBuf>, metas: &mut Vec<FileMeta>) -> Result<bool, Fatal> {
        self.update(|j| j.done += 1);
        let path = match res {
            Ok(path) => path,
            Err(ImportError::LoginRequired) => {
                return Err(Fatal { code: "bambu_login_required", message: "Bambu rejected the login. Log in again and retry.".into() });
            }
            Err(e) => {
                tracing::warn!("Import job {}: {name} failed: {e}", self.job_id);
                self.update(|j| j.warnings.push(format!("{name}: {e}")));
                return Ok(false);
            }
        };
        match crate::bundles::file_meta(path.clone(), name).await {
            Ok(meta) => {
                metas.push(meta);
                Ok(true)
            }
            Err(e) => {
                self.update(|j| j.warnings.push(format!("{name}: {e}")));
                Ok(false)
            }
        }
    }
}

