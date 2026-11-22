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

    let state = state::AppState {
        db: Arc::new(db),
        sync,
        paths: Arc::new(paths),
        http: lima_importers::client(),
        imports: Default::default(),
        auto_backup,
        backup_keep,
    };

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
        .route("/projects/{project_id}", delete(routes::project_delete::project_delete))
        .route("/projects/{project_id}", get(routes::project_detail::project_detail))
        .route("/projects/{project_id}", patch(routes::project_update::project_update))
        .route("/projects/{project_id}/favourite", put(routes::project_favourite::add_favourite))
        .route("/projects/{project_id}/favourite", delete(routes::project_favourite::remove_favourite))
        .route("/projects/{project_id}/download", get(routes::project_download::project_download))
        .route("/projects/{project_id}/import", post(routes::project_import::project_import))
        .route("/projects/{project_id}/assets/{asset_id}", delete(routes::assets::delete::asset_delete))
        
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
        .route("/ignored-folders", delete(routes::ignored_folders::unignore_folder));
    
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
