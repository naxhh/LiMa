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
    /// Collection imports and syncs in progress (or finished in the last hour).
    pub collection_jobs: crate::collection_sync::CollectionJobs,
    /// How often synced collections are checked (`LIMA_COLLECTION_SYNC_HOURS`, 0 = never).
    pub collection_sync_hours: u64,
    /// Automatic metadata backups; `None` when disabled.
    pub auto_backup: Option<crate::backups::AutoBackup>,
    /// How many backup files to keep.
    pub backup_keep: usize,
    /// Days deleted projects stay in the trash; 0 = no trash, deletes are final.
    pub trash_days: u64,
    /// Thingiverse app token (`LIMA_THINGIVERSE_TOKEN`); Thingiverse imports need it.
    pub thingiverse_token: Option<String>,
}
