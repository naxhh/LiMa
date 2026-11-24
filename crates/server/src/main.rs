use std::{env, net::SocketAddr};

use tokio::net::TcpListener;

mod app;
mod archive;
mod backups;
mod bundles;
mod collection_sync;
mod models;
mod openapi;
mod routes;
mod state;
mod trash;
mod uploads;
#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_port = env::var("LIMA_SERVER_PORT").unwrap_or_else(|_| "6767".to_string());
    let paths = lima_domain::config::Paths::from_env();
    for dir in paths.required_dirs() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    }
    tracing::info!("Library: {} | State: {} | UI: {}", paths.library.display(), paths.state.display(), paths.ui.display());

    let db = lima_db::Db::connect(&paths.database_url).await?;
    db.migrate().await?;
    lima_db::queries::collections::ensure_favourites(db.pool(), &backups::now()).await?;

    let settings = state::Settings::from_env();
    let state = state::AppState::new(db, paths, &settings).await?;
    start_background_tasks(&state, &settings).await;

    let addr: SocketAddr = format!("0.0.0.0:{server_port}").parse()?;
    tracing::info!("LIMA server started in port {server_port}");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app::router(state)).await?;
    Ok(())
}

/// The watcher, automatic backups, trash purging and collection syncs.
async fn start_background_tasks(state: &state::AppState, settings: &state::Settings) {
    uploads::spawn_cleanup(state.clone());

    if settings.watcher {
        state.sync.start_watcher(lima_indexer::sync::WatcherConfig::default());
    } else {
        tracing::info!("Library watcher disabled (LIMA_WATCHER)");
    }

    match settings.auto_backup {
        Some(cfg) => {
            tracing::info!("Metadata backups every {}h into {} (keeping {})", cfg.interval.as_secs() / 3600, state.paths.backups.display(), cfg.keep);
            backups::spawn_auto_backup(state.db.pool().clone(), state.paths.backups.clone(), cfg);
        }
        None => tracing::info!("Automatic metadata backups disabled (LIMA_BACKUP_INTERVAL_HOURS=0)"),
    }

    if settings.trash_days > 0 {
        tracing::info!("Deleted projects stay in the trash for {} days", settings.trash_days);
        trash::spawn_auto_purge(state.clone(), settings.trash_days);
    } else {
        tracing::info!("Trash disabled (LIMA_TRASH_DAYS=0): deletes are final");
    }

    if let Err(e) = lima_db::queries::collection_sources::fail_interrupted_runs(state.db.pool(), &backups::now()).await {
        tracing::warn!("Could not close interrupted collection syncs: {e}");
    }
    if settings.collection_sync_hours > 0 {
        collection_sync::spawn_scheduler(state.clone(), settings.collection_sync_hours);
    }
}
