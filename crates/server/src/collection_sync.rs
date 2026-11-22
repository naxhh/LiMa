//! Collections imported from Printables, MakerWorld or Thingiverse, and keeping them in sync.
//!
//! A run reads the source collection and brings the LiMa collection in line with it:
//! - new models are imported as projects (or linked, when the library already has a project
//!   from that model page) and added to the collection;
//! - models that left the source are deleted (to the trash) when this collection imported
//!   them, or only taken out of the collection when the project was already in the library;
//! - models the user left unticked, or whose project they deleted, are never imported again.
//!
//! Synced collections are checked every `LIMA_COLLECTION_SYNC_HOURS` (default 12, 0 = off).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lima_db::queries::collection_sources::{self as db, ItemRow, SourceRow};
use lima_importers::collections::{CollectionRef, RemoteCollection};
use lima_importers::{ModelRef, Source, Tokens};
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::routes::imports::{ModelImportError, import_model_now, now_rfc3339};
use crate::state::AppState;

pub const DEFAULT_HOURS: u64 = 12;
/// `integration_tokens` name of the Bambu token used by synced MakerWorld collections.
pub const BAMBU_TOKEN: &str = "bambu";
const KEEP_FINISHED_SECS: i64 = 3600;

pub fn hours_from_env() -> u64 {
    std::env::var("LIMA_COLLECTION_SYNC_HOURS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_HOURS)
}

pub type CollectionJobs = Arc<Mutex<HashMap<String, CollectionJob>>>;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CollectionJob {
    pub id: String,
    pub collection_id: String,
    /// `running`, `complete` or `failed`.
    pub status: String,
    /// Models to import or link in this run.
    pub total: usize,
    pub done: usize,
    pub imported: usize,
    /// Already in the library: added to the collection without downloading.
    pub linked: usize,
    /// Models that left the source collection.
    pub removed: usize,
    pub failed: Vec<FailedModel>,
    /// Model being imported.
    pub current: Option<String>,
    /// Why the whole run stopped (e.g. the Bambu login expired).
    pub error: Option<String>,
    pub error_code: Option<String>,
    #[serde(skip)]
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FailedModel {
    pub name: String,
    pub error: String,
}

pub fn source_name(source: Source) -> &'static str {
    match source {
        Source::Printables => "printables",
        Source::Makerworld => "makerworld",
        Source::Thingiverse => "thingiverse",
    }
}

fn parse_source(name: &str) -> Option<Source> {
    match name {
        "makerworld" => Some(Source::Makerworld),
        "thingiverse" => Some(Source::Thingiverse),
        "printables" => Some(Source::Printables),
        _ => None,
    }
}

/// The running job for a collection, if any.
pub fn running_job(state: &AppState, collection_id: &str) -> Option<String> {
    state.collection_jobs.lock().unwrap().values().find(|j| j.collection_id == collection_id && j.status == "running").map(|j| j.id.clone())
}

pub fn job(state: &AppState, id: &str) -> Option<CollectionJob> {
    state.collection_jobs.lock().unwrap().get(id).cloned()
}

fn update(state: &AppState, job_id: &str, f: impl FnOnce(&mut CollectionJob)) {
    if let Some(job) = state.collection_jobs.lock().unwrap().get_mut(job_id) {
        f(job);
    }
}

/// Starts a run in the background (or returns the one already running). `selected` limits a
/// first import to those model ids; the others are remembered as excluded.
pub fn start(state: &AppState, source: SourceRow, selected: Option<HashSet<String>>, bambu_token: Option<String>) -> String {
    let mut jobs = state.collection_jobs.lock().unwrap();
    if let Some(j) = jobs.values().find(|j| j.collection_id == source.collection_id && j.status == "running") {
        return j.id.clone();
    }
    let cutoff = OffsetDateTime::now_utc().unix_timestamp() - KEEP_FINISHED_SECS;
    jobs.retain(|_, j| j.finished_at.is_none_or(|t| t > cutoff));
    let id = uuid::Uuid::new_v4().to_string();
    jobs.insert(
        id.clone(),
        CollectionJob {
            id: id.clone(),
            collection_id: source.collection_id.clone(),
            status: "running".into(),
            total: 0,
            done: 0,
            imported: 0,
            linked: 0,
            removed: 0,
            failed: vec![],
            current: None,
            error: None,
            error_code: None,
            finished_at: None,
        },
    );
    drop(jobs);
    let (state, job_id) = (state.clone(), id.clone());
    tokio::spawn(async move {
        let result = run(&state, &source, &job_id, selected, bambu_token).await;
        let now = now_rfc3339();
        let error = result.as_ref().err().map(|e| e.message.clone());
        if let Err(e) = db::set_result(state.db.pool(), &source.id, &now, error.as_deref()).await {
            tracing::warn!("Could not record the sync result of collection {}: {e}", source.collection_id);
        }
        update(&state, &job_id, |j| {
            j.current = None;
            j.finished_at = Some(OffsetDateTime::now_utc().unix_timestamp());
            match result {
                Ok(()) => j.status = "complete".into(),
                Err(e) => {
                    j.status = "failed".into();
                    j.error = Some(e.message);
                    j.error_code = Some(e.code.into());
                }
            }
        });
    });
    id
}

#[derive(Debug)]
pub struct RunError {
    pub code: &'static str,
    pub message: String,
}

impl RunError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        RunError { code, message: message.into() }
    }
}

async fn fetch(state: &AppState, source: Source, id: &str) -> Result<RemoteCollection, RunError> {
    let tokens = Tokens { thingiverse: state.thingiverse_token.as_deref(), ..Default::default() };
    lima_importers::collections::fetch_collection(&state.http, &CollectionRef { source, id: id.to_string() }, tokens)
        .await
        .map_err(|e| RunError::new("fetch_failed", format!("Couldn't read the collection: {e}")))
}

async fn project_exists(state: &AppState, id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE id = ?1").bind(id).fetch_one(state.db.pool()).await.unwrap_or(0) > 0
}

async fn add_to_collection(state: &AppState, collection_id: &str, project_id: &str) {
    if let Err(e) = lima_db::queries::collections::add_project(state.db.pool(), collection_id, project_id, &now_rfc3339()).await {
        tracing::warn!("Could not add project {project_id} to collection {collection_id}: {e:?}");
    }
}

async fn run(state: &AppState, s: &SourceRow, job_id: &str, selected: Option<HashSet<String>>, bambu_token: Option<String>) -> Result<(), RunError> {
    let pool = state.db.pool();
    let source = parse_source(&s.source).ok_or_else(|| RunError::new("unsupported", format!("Unknown source {}", s.source)))?;
    let remote = fetch(state, source, &s.source_id).await?;
    let db_err = |e: sqlx::Error| RunError::new("db_failure", e.to_string());
    let known: HashMap<String, ItemRow> = db::items(pool, &s.id).await.map_err(db_err)?.into_iter().map(|i| (i.model_id.clone(), i)).collect();

    // What to do with each model in the source.
    let mut todo = Vec::new();
    for m in &remote.models {
        match known.get(&m.id) {
            Some(item) if item.excluded => {}
            Some(item) => match &item.project_id {
                Some(pid) if project_exists(state, pid).await => add_to_collection(state, &s.collection_id, pid).await,
                // The user deleted the project this collection imported: don't bring it back.
                _ if item.imported => {
                    let excluded = ItemRow { project_id: None, excluded: true, ..item.clone() };
                    db::upsert_item(pool, &s.id, &excluded).await.map_err(db_err)?;
                }
                // Failed last time: try again.
                _ => todo.push(m.clone()),
            },
            None if selected.as_ref().is_some_and(|sel| !sel.contains(&m.id)) => {
                let item = ItemRow { model_id: m.id.clone(), model_url: m.url.clone(), name: m.name.clone(), project_id: None, imported: false, excluded: true, last_error: None };
                db::upsert_item(pool, &s.id, &item).await.map_err(db_err)?;
            }
            None => todo.push(m.clone()),
        }
    }
    update(state, job_id, |j| j.total = todo.len());

    for m in todo {
        update(state, job_id, |j| j.current = Some(m.name.clone()));
        let mut item = ItemRow { model_id: m.id.clone(), model_url: m.url.clone(), name: m.name.clone(), project_id: None, imported: false, excluded: false, last_error: None };

        if let Some(pid) = db::project_for_url(pool, &m.url).await.map_err(db_err)? {
            add_to_collection(state, &s.collection_id, &pid).await;
            item.project_id = Some(pid);
            db::upsert_item(pool, &s.id, &item).await.map_err(db_err)?;
            update(state, job_id, |j| {
                j.linked += 1;
                j.done += 1;
            });
            continue;
        }

        let result = async {
            let model = lima_importers::fetch_model(&state.http, &ModelRef { source, id: m.id.clone() }, Tokens { thingiverse: state.thingiverse_token.as_deref(), ..Default::default() })
                .await
                .map_err(|e| ModelImportError::Other(e.to_string()))?;
            let files = lima_importers::collections::default_files(&model);
            let token = if source.downloads_need_login() && !files.is_empty() {
                Some(bambu_token.clone().ok_or(ModelImportError::LoginRequired)?)
            } else {
                None
            };
            import_model_now(state, model, files, token).await
        }
        .await;

        match result {
            Ok(pid) => {
                add_to_collection(state, &s.collection_id, &pid).await;
                item.project_id = Some(pid);
                item.imported = true;
                db::upsert_item(pool, &s.id, &item).await.map_err(db_err)?;
                update(state, job_id, |j| {
                    j.imported += 1;
                    j.done += 1;
                });
            }
            Err(ModelImportError::LoginRequired) => {
                return Err(RunError::new("bambu_login_required", "MakerWorld needs a Bambu login to download files. Log in and sync again."));
            }
            Err(ModelImportError::Other(e)) => {
                tracing::warn!("Collection {}: importing {} failed: {e}", s.collection_id, m.url);
                item.last_error = Some(e.clone());
                db::upsert_item(pool, &s.id, &item).await.map_err(db_err)?;
                update(state, job_id, |j| {
                    j.failed.push(FailedModel { name: m.name.clone(), error: e });
                    j.done += 1;
                });
            }
        }
    }

    // Models that left the source. An empty answer is more likely a glitch than an emptied
    // collection, so it never removes anything.
    if remote.models.is_empty() && !known.is_empty() {
        return Err(RunError::new("empty_source", "The site returned no models; nothing was removed."));
    }
    if remote.truncated {
        return Ok(());
    }
    let present: HashSet<&str> = remote.models.iter().map(|m| m.id.as_str()).collect();
    for item in known.values().filter(|i| !present.contains(i.model_id.as_str())) {
        if let Some(pid) = &item.project_id {
            if item.imported {
                remove_project(state, pid).await;
            } else if let Err(e) = lima_db::queries::collections::remove_project(pool, &s.collection_id, pid, &now_rfc3339()).await {
                tracing::warn!("Could not take {pid} out of collection {}: {e:?}", s.collection_id);
            }
            update(state, job_id, |j| j.removed += 1);
        }
        db::remove_item(pool, &s.id, &item.model_id).await.map_err(db_err)?;
    }
    Ok(())
}

/// Deletes a project this collection imported: to the trash when it's on.
async fn remove_project(state: &AppState, project_id: &str) {
    let result = if state.trash_days > 0 {
        crate::trash::trash_project(state, project_id).await.map(|_| ()).map_err(|e| format!("{e:?}"))
    } else {
        lima_db::queries::projects_delete::delete_project(state.db.pool(), &state.paths, project_id).await.map_err(|e| format!("{e:?}"))
    };
    if let Err(e) = result {
        tracing::warn!("Could not remove project {project_id} after it left its source collection: {e}");
    }
}

/// Runs every synced collection now and then every `hours`.
pub fn spawn_scheduler(state: AppState, hours: u64) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        let mut tick = tokio::time::interval(Duration::from_secs(hours * 3600));
        loop {
            tick.tick().await;
            let sources = match db::synced(state.db.pool()).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Could not list synced collections: {e}");
                    continue;
                }
            };
            for s in sources {
                let token = if s.source == "makerworld" { db::token(state.db.pool(), BAMBU_TOKEN).await.ok().flatten() } else { None };
                let collection = s.collection_id.clone();
                let job_id = start(&state, s, None, token);
                // One at a time, so the sites aren't hammered.
                while job(&state, &job_id).is_some_and(|j| j.status == "running") {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                if let Some(j) = job(&state, &job_id) {
                    tracing::info!(
                        "Synced collection {collection}: {} imported, {} linked, {} removed, {} failed{}",
                        j.imported,
                        j.linked,
                        j.removed,
                        j.failed.len(),
                        j.error.map(|e| format!(" ({e})")).unwrap_or_default()
                    );
                }
            }
        }
    });
}
