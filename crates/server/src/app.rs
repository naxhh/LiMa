//! The HTTP app: API routes, media, the UI and the OpenAPI docs.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{delete, get, get_service, patch, post, put},
};
use tower::ServiceBuilder;
use tower_http::{catch_panic::CatchPanicLayer, services::{ServeDir, ServeFile}};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::routes;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let ui_dir = ServeDir::new(&state.paths.ui).fallback(ServeFile::new(state.paths.ui.join("index.html")));
    let library_dir = ServeDir::new(&state.paths.library);
    let thumbs_dir = ServeDir::new(state.paths.thumbnails());

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
        .route("/projects/ids", get(routes::project::list_project_ids))
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
    
    Router::new()
        .nest("/api", api)
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .layer(
            ServiceBuilder::new()
                .layer(CatchPanicLayer::new())
        )
        .nest_service("/media/library", get_service(library_dir).handle_error(|_| async { StatusCode::NOT_FOUND }))
        .nest_service("/media/thumbs", get_service(thumbs_dir).handle_error(|_| async { StatusCode::NOT_FOUND }))
        .fallback_service(get_service(ui_dir).handle_error(|_| async { StatusCode::NOT_FOUND }))
}
