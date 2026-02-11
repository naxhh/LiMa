use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, delete, patch, get_service},
    http::StatusCode,
};
use tower::ServiceBuilder;
use tower_http::{catch_panic::CatchPanicLayer, services::{ServeDir, ServeFile}};
use std::{env, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod routes;
mod state;
mod models;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_port = env::var("LIMA_SERVER_PORT").unwrap_or_else(|_| "6767".to_string());
    let database_url = env::var("LIMA_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:data/state/lima.db".to_string());

    let db = lima_db::Db::connect(&database_url).await?;
    db.migrate().await?;

    let ui_dir = ServeDir::new("ui/dist").fallback(ServeFile::new("ui/dist/index.html"));
    let library_dir = ServeDir::new("data/library");
    let thumbs_dir = ServeDir::new("data/state/thumbnails");

    let state = state::AppState {
        db: Arc::new(db),
    };

    let api = Router::new()
        .route("/health", get(routes::health::health_check))
        .route("/projects", get(routes::project::list_projects))
        .route ("/projects", post(routes::project_create::create_project))
        .route("/projects/{project_id}", delete(routes::project_delete::project_delete))
        .route("/projects/{project_id}", get(routes::project_detail::project_detail))
        .route("/projects/{project_id}", patch(routes::project_update::project_update))
        .route("/projects/{project_id}/import", post(routes::project_import::project_import))
        .route("/projects/{project_id}/assets/{asset_id}", delete(routes::assets::delete::asset_delete))
        
        .route("/tags", get(routes::tags::list::list_tags))
        .route("/tags", post(routes::tags::create::create_tag))
        
        .route("/bundles", post(routes::bundle_create::create_bundle)
            .route_layer(DefaultBodyLimit::disable()),
        )
        .route("/bundles/{bundle_id}", delete(routes::bundle_delete::bundle_delete));
    
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
        routes::project::list_projects,
        routes::project_create::create_project,
        routes::project_delete::project_delete,
        routes::project_detail::project_detail,
        routes::project_update::project_update,
        routes::project_import::project_import,

        routes::assets::delete::asset_delete,

        routes::tags::list::list_tags,
        routes::tags::create::create_tag,

        routes::bundle_create::create_bundle,
        routes::bundle_delete::bundle_delete,
    ),
    components(schemas(
        crate::models::http_error::ApiErrorBody,
        routes::health::HealthResponse,

        routes::project::ListProjectsResponse,
        routes::project::ListProjectsParams,
        routes::project_create::CreateProjectRequest,
        routes::project_create::CreateProjectResponse,
        routes::project_update::PatchProjectRequest,
        routes::project_import::ImportProjectRequest,

        routes::project_detail::ProjectDetailResponse,
        routes::project_detail::ProjectAssetResponse,
        routes::project_detail::ProjectTagResponse,

        routes::tags::list::ListTagsResponse,
        routes::tags::list::ListTagsParams,
        routes::tags::list::Tag,

        routes::tags::create::CreateTagRequest,
        routes::tags::create::CreateTagResponse,
        
        routes::bundle_create::CreateBundleResponse,
    )),
)]
pub struct ApiDoc;
