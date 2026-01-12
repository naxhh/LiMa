use axum::{routing::get, Router};
use std::{env, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

mod routes;
mod state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_port = env::var("LIMA_SERVER_PORT").unwrap_or_else(|_| "6767".to_string());
    let database_url = env::var("LIMA_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:data/state/lima.db".to_string());

    let db = lima_db::Db::connect(&database_url).await?;
    db.migrate().await?;

    let state = state::AppState {
        db: Arc::new(db),
    };

    let app = Router::new()
        .route("/health", get(routes::health::health_check))
        .with_state(state);

    let addr: SocketAddr = format!("0.0.0.0:{}", server_port).parse()?;

    tracing::info!("LIMA server started in port {server_port}");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
