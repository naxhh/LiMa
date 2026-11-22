use lima_db::Db;
use lima_domain::config::Paths;
use lima_indexer::sync::SyncManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub sync: SyncManager,
    pub paths: Arc<Paths>,
    /// Outgoing HTTP client for URL imports and the Bambu login.
    pub http: lima_importers::HttpClient,
    pub imports: crate::routes::imports::ImportJobs,
}
