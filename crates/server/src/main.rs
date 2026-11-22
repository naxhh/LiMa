use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, put, delete, patch, get_service},
    http::StatusCode,
};
use tower::ServiceBuilder;
use tower_http::{catch_panic::CatchPanicLayer, services::{ServeDir, ServeFile}};
use std::{env, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod backups;
mod archive;
mod collection_sync;
mod trash;
mod routes;
mod state;
mod models;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_port = env::var("LIMA_SERVER_PORT").unwrap_or_else(|_| "6767".to_string());
    let paths = lima_domain::config::Paths::from_env();
    for dir in paths.required_dirs() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    }
    tracing::info!(
        "Library: {} | State: {} | UI: {}",
        paths.library.display(),
        paths.state.display(),
        paths.ui.display()
    );

    let db = lima_db::Db::connect(&paths.database_url).await?;
    db.migrate().await?;
    let started = time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    lima_db::queries::collections::ensure_favourites(db.pool(), &started).await?;

    let ui_dir = ServeDir::new(&paths.ui).fallback(ServeFile::new(paths.ui.join("index.html")));
    let library_dir = ServeDir::new(&paths.library);
    let thumbs_dir = ServeDir::new(paths.thumbnails());

    let sync = lima_indexer::sync::SyncManager::new(db.pool().clone(), &paths.library, paths.thumbnails()).await?;

    // Filesystem watcher: on by default, LIMA_WATCHER=false to disable.
    let watcher_enabled = !matches!(
        env::var("LIMA_WATCHER").unwrap_or_default().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    );
    if watcher_enabled {
        sync.start_watcher(lima_indexer::sync::WatcherConfig::default());
    } else {
        tracing::info!("Library watcher disabled (LIMA_WATCHER)");
    }

    let auto_backup = backups::AutoBackup::from_env();
    let backup_keep = auto_backup.map(|a| a.keep).unwrap_or(backups::DEFAULT_KEEP);
    match auto_backup {
        Some(cfg) => {
            tracing::info!(
                "Metadata backups every {}h into {} (keeping {})",
                cfg.interval.as_secs() / 3600,
                paths.backups.display(),
                cfg.keep
            );
            backups::spawn_auto_backup(db.pool().clone(), paths.backups.clone(), cfg);
        }
        None => tracing::info!("Automatic metadata backups disabled (LIMA_BACKUP_INTERVAL_HOURS=0)"),
    }

    let trash_days = trash::days_from_env();
    let collection_sync_hours = collection_sync::hours_from_env();
    if trash_days > 0 {
        tracing::info!("Deleted projects stay in the trash for {trash_days} days");
    } else {
        tracing::info!("Trash disabled (LIMA_TRASH_DAYS=0): deletes are final");
    }

    let state = state::AppState {
        db: Arc::new(db),
        sync,
        paths: Arc::new(paths),
        http: lima_importers::client(),
        imports: Default::default(),
        collection_jobs: Default::default(),
        collection_sync_hours,
        auto_backup,
        backup_keep,
        trash_days,
        thingiverse_token: std::env::var("LIMA_THINGIVERSE_TOKEN").ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
    };
    if trash_days > 0 {
        trash::spawn_auto_purge(state.clone(), trash_days);
    }
    if let Err(e) = lima_db::queries::collection_sources::fail_interrupted_runs(state.db.pool(), &crate::backups::now()).await {
        tracing::warn!("Could not close interrupted collection syncs: {e}");
    }
    if collection_sync_hours > 0 {
        collection_sync::spawn_scheduler(state.clone(), collection_sync_hours);
    }

    let api = Router::new()
        .route("/health", get(routes::health::health_check))
        .route("/stats", get(routes::stats::library_stats))
        .route("/metadata/export", get(routes::metadata::export_metadata))
        .route(
            "/metadata/restore",
            post(routes::metadata::restore_upload).route_layer(DefaultBodyLimit::max(256 * 1024 * 1024)),
        )
        .route("/metadata/backups", get(routes::metadata::list_backups))
        .route("/metadata/backups", post(routes::metadata::create_backup))
        .route("/metadata/backups/{name}", get(routes::metadata::download_backup))
        .route("/metadata/backups/{name}/restore", post(routes::metadata::restore_backup))
        .route("/projects", get(routes::project::list_projects))
        .route ("/projects", post(routes::project_create::create_project))
        .route("/projects/bulk", post(routes::project_bulk::bulk))
        .route("/projects/download", post(routes::project_download::download_many))
        .route("/projects/{project_id}", delete(routes::project_delete::project_delete))
        .route("/projects/{project_id}", get(routes::project_detail::project_detail))
        .route("/projects/{project_id}", patch(routes::project_update::project_update))
        .route("/projects/{project_id}/favourite", put(routes::project_favourite::add_favourite))
        .route("/projects/{project_id}/favourite", delete(routes::project_favourite::remove_favourite))
        .route("/projects/{project_id}/download", get(routes::project_download::project_download))
        .route("/projects/{project_id}/import", post(routes::project_import::project_import))
        .route("/projects/{project_id}/assets/{asset_id}", delete(routes::assets::delete::asset_delete))
        .route("/projects/{project_id}/assets/{asset_id}/zip", get(routes::assets::zip::zip_contents))
        .route("/projects/{project_id}/assets/{asset_id}/zip/entry", get(routes::assets::zip::zip_entry))
        
        .route("/tags", get(routes::tags::list::list_tags))
        .route("/tags", post(routes::tags::create::create_tag))
        .route("/tags/{tag_id}", patch(routes::tags::update::update_tag))
        .route("/tags/{tag_id}", delete(routes::tags::delete::delete_tag))
        
        .route("/collections", get(routes::collections::list::list_collections))
        .route("/collections", post(routes::collections::create::create_collection))
        .route("/collections/{collection_id}", get(routes::collections::detail::collection_detail))
        .route("/collections/{collection_id}", patch(routes::collections::update::update_collection))
        .route("/collections/{collection_id}", delete(routes::collections::delete::delete_collection))
        .route("/collections/{collection_id}/projects", post(routes::collections::membership::add_project))
        .route("/collections/{collection_id}/projects/{project_id}", delete(routes::collections::membership::remove_project))

        .route("/bundles", post(routes::bundle_create::create_bundle)
            .route_layer(DefaultBodyLimit::disable()),
        )
        .route("/bundles/{bundle_id}", delete(routes::bundle_delete::bundle_delete))

        .route("/imports", post(routes::imports::start_import))
        .route("/imports/preview", post(routes::imports::import_preview))
        .route("/imports/{job_id}", get(routes::imports::import_status))
        .route("/imports/collections", post(routes::collection_import::start_collection_import))
        .route("/imports/collections/preview", post(routes::collection_import::collection_preview))
        .route("/imports/collections/{job_id}", get(routes::collection_import::collection_job_status))
        .route("/collections/{collection_id}/source", get(routes::collection_import::get_collection_source).patch(routes::collection_import::update_collection_source))
        .route("/collections/{collection_id}/source/sync", post(routes::collection_import::sync_collection_now))
        .route("/collection-syncs", get(routes::collection_syncs::list_collection_syncs))
        .route("/collection-syncs/runs/{run_id}", get(routes::collection_syncs::get_collection_sync_run))
        .route("/sync/history", get(routes::collection_syncs::sync_history))
        .route("/integrations/bambu/status", get(routes::bambu::bambu_status))
        .route("/integrations/bambu/login", post(routes::bambu::bambu_login))
        .route("/integrations/bambu/login/send-code", post(routes::bambu::bambu_send_code))
        .route("/integrations/bambu/login/code", post(routes::bambu::bambu_login_code))
        .route("/integrations/bambu/login/tfa", post(routes::bambu::bambu_login_tfa))
        .route("/integrations/bambu/token", post(routes::bambu::bambu_set_token))
        .route("/integrations/bambu/logout", post(routes::bambu::bambu_logout))

        .route("/sync/run", post(routes::sync::start_sync))
        .route("/sync/status", get(routes::sync::sync_status))
        .route("/sync/runs", get(routes::sync::list_sync_runs))
        .route("/sync/runs/{run_id}", get(routes::sync::get_sync_run))
        .route("/sync/runs/{run_id}/events", get(routes::sync::list_sync_events))
        .route("/ignored-folders", get(routes::ignored_folders::list_ignored_folders))
        .route("/ignored-folders", delete(routes::ignored_folders::unignore_folder))
        .route("/trash", get(routes::trash::list_trash))
        .route("/trash", delete(routes::trash::empty_trash))
        .route("/trash/{trash_id}", delete(routes::trash::delete_trash_item))
        .route("/trash/{trash_id}/restore", post(routes::trash::restore_trash))
        .route("/duplicates", get(routes::duplicates::list_duplicates))
        .route("/duplicates/resolve", post(routes::duplicates::resolve_duplicates))
        .route("/duplicates/dismiss", post(routes::duplicates::dismiss_duplicates))
        .route("/duplicates/dismissed", delete(routes::duplicates::clear_dismissed));
    
    let app = Router::new()
        .nest("/api", api)
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .layer(
            ServiceBuilder::new()
                .layer(CatchPanicLayer::new())
        )
        .nest_service("/media/library", get_service(library_dir).handle_error(|_| async { StatusCode::NOT_FOUND }))
        .nest_service("/media/thumbs", get_service(thumbs_dir).handle_error(|_| async { StatusCode::NOT_FOUND }))
        .fallback_service(get_service(ui_dir).handle_error(|_| async { StatusCode::NOT_FOUND }));


    let addr: SocketAddr = format!("0.0.0.0:{}", server_port).parse()?;

    tracing::info!("LIMA server started in port {server_port}");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::health::health_check,
        routes::stats::library_stats,
        routes::metadata::export_metadata,
        routes::metadata::restore_upload,
        routes::metadata::list_backups,
        routes::metadata::create_backup,
        routes::metadata::download_backup,
        routes::metadata::restore_backup,
        routes::project::list_projects,
        routes::project_create::create_project,
        routes::project_delete::project_delete,
        routes::project_detail::project_detail,
        routes::project_update::project_update,
        routes::project_import::project_import,
        routes::project_download::project_download,
        routes::project_download::download_many,
        routes::project_bulk::bulk,
        routes::project_favourite::add_favourite,
        routes::project_favourite::remove_favourite,

        routes::assets::delete::asset_delete,

        routes::tags::list::list_tags,
        routes::tags::create::create_tag,
        routes::tags::update::update_tag,
        routes::tags::delete::delete_tag,

        routes::collections::list::list_collections,
        routes::collections::create::create_collection,
        routes::collections::detail::collection_detail,
        routes::collections::update::update_collection,
        routes::collections::delete::delete_collection,
        routes::collections::membership::add_project,
        routes::collections::membership::remove_project,

        routes::bundle_create::create_bundle,
        routes::bundle_delete::bundle_delete,

        routes::imports::import_preview,
        routes::imports::start_import,
        routes::imports::import_status,
        routes::collection_import::collection_preview,
        routes::collection_import::start_collection_import,
        routes::collection_import::collection_job_status,
        routes::collection_import::get_collection_source,
        routes::collection_import::update_collection_source,
        routes::collection_import::sync_collection_now,
        routes::collection_syncs::list_collection_syncs,
        routes::collection_syncs::get_collection_sync_run,
        routes::collection_syncs::sync_history,
        routes::bambu::bambu_status,
        routes::bambu::bambu_login,
        routes::bambu::bambu_send_code,
        routes::bambu::bambu_login_code,
        routes::bambu::bambu_login_tfa,
        routes::bambu::bambu_set_token,
        routes::bambu::bambu_logout,

        routes::sync::start_sync,
        routes::sync::sync_status,
        routes::sync::list_sync_runs,
        routes::sync::get_sync_run,
        routes::sync::list_sync_events,
        routes::ignored_folders::list_ignored_folders,
        routes::ignored_folders::unignore_folder,
        routes::trash::list_trash,
        routes::trash::restore_trash,
        routes::trash::delete_trash_item,
        routes::trash::empty_trash,
        routes::assets::zip::zip_contents,
        routes::assets::zip::zip_entry,
        routes::duplicates::list_duplicates,
        routes::duplicates::resolve_duplicates,
        routes::duplicates::dismiss_duplicates,
        routes::duplicates::clear_dismissed,
    ),
    components(schemas(
        crate::models::http_error::ApiErrorBody,
        routes::health::HealthResponse,

        routes::project::ListProjectsResponse,
        routes::project::ProjectListItem,
        routes::stats::LibraryStatsResponse,
        routes::metadata::ListBackupsResponse,
        crate::backups::BackupFile,
        lima_domain::models::metadata::MetadataBackup,
        lima_domain::models::metadata::BackupTag,
        lima_domain::models::metadata::BackupCollection,
        lima_domain::models::metadata::BackupProject,
        lima_domain::models::metadata::BackupMembership,
        lima_domain::models::metadata::BackupSource,
        lima_domain::models::metadata::RestoreReport,
        lima_domain::models::metadata::RenamedMatch,
        routes::project::ListProjectsParams,
        routes::ignored_folders::ListIgnoredFoldersResponse,
        routes::ignored_folders::IgnoredFolderItem,
        routes::trash::ListTrashResponse,
        routes::project_bulk::BulkRequest,
        routes::project_bulk::BulkAction,
        routes::project_bulk::BulkResponse,
        routes::project_download::DownloadManyForm,
        routes::trash::TrashItem,
        routes::trash::RestoreTrashResponse,
        routes::project_import::ImportProjectResponse,
        archive::ExtractSummary,
        routes::assets::zip::ZipContentsResponse,
        routes::assets::zip::ZipEntry,
        routes::duplicates::DuplicatesResponse,
        routes::duplicates::DuplicateProjectGroup,
        routes::duplicates::DuplicateProject,
        routes::duplicates::SharedFileGroup,
        routes::duplicates::SharedFileCopy,
        routes::duplicates::RemoveMode,
        routes::duplicates::ResolveDuplicatesRequest,
        routes::duplicates::ResolveDuplicatesResponse,
        routes::duplicates::MergedMetadata,
        routes::duplicates::DismissDuplicatesRequest,
        routes::project_delete::DeleteProjectResponse,
        routes::project_create::CreateProjectRequest,
        routes::project_create::CreateProjectResponse,
        routes::project_update::PatchProjectRequest,
        routes::project_import::ImportProjectRequest,

        routes::project_detail::ProjectDetailResponse,
        routes::project_detail::ProjectAssetResponse,
        routes::project_detail::ProjectTagResponse,
        routes::project_detail::ProjectCollectionResponse,

        routes::tags::list::ListTagsResponse,
        routes::tags::list::ListTagsParams,
        routes::tags::list::Tag,

        routes::tags::create::CreateTagRequest,
        routes::tags::update::PatchTagRequest,
        
        routes::collections::Collection,
        routes::collections::CollectionCover,
        routes::collections::list::ListCollectionsResponse,
        routes::collections::detail::CollectionDetailResponse,
        routes::collections::create::CreateCollectionRequest,
        routes::collections::update::PatchCollectionRequest,
        routes::collections::membership::AddCollectionProjectRequest,

        routes::bundle_create::CreateBundleResponse,

        routes::imports::ImportPreviewRequest,
        routes::imports::ImportPreviewResponse,
        routes::imports::ImportImage,
        routes::imports::ImportFile,
        routes::imports::StartImportRequest,
        routes::imports::StartImportResponse,
        routes::imports::ImportJobResponse,
        routes::collection_import::CollectionPreviewRequest,
        routes::collection_import::CollectionPreviewResponse,
        routes::collection_import::CollectionPreviewModel,
        routes::collection_import::StartCollectionImportRequest,
        routes::collection_import::StartCollectionImportResponse,
        routes::collection_import::CollectionSourceResponse,
        routes::collection_import::UpdateCollectionSourceRequest,
        routes::collection_import::StartSyncResponse,
        collection_sync::CollectionJob,
        collection_sync::FailedModel,
        routes::collection_syncs::CollectionSyncItem,
        routes::collection_syncs::CollectionSyncsResponse,
        routes::collection_syncs::CollectionSyncRun,
        routes::collection_syncs::SyncHistoryItem,
        routes::collection_syncs::SyncHistoryResponse,
        routes::bambu::BambuStatusResponse,
        routes::bambu::BambuLoginRequest,
        routes::bambu::BambuSendCodeRequest,
        routes::bambu::BambuCodeRequest,
        routes::bambu::BambuTfaRequest,
        routes::bambu::BambuTokenRequest,
        routes::bambu::BambuLoginResponse,

        routes::sync::StartSyncRequest,
        routes::sync::SyncStatusResponse,
        routes::sync::ListSyncRunsParams,
        routes::sync::ListSyncRunsResponse,
        routes::sync::ListSyncEventsParams,
        routes::sync::ListSyncEventsResponse,
        lima_domain::models::sync::SyncRunRow,
        lima_domain::models::sync::SyncStats,
        lima_domain::models::sync::SyncEventRow,
        lima_domain::models::sync::SyncOptions,
        lima_domain::models::sync::WatcherStatus,
    )),
)]
pub struct ApiDoc;
