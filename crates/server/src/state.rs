use lima_db::Db;
use lima_domain::config::Paths;
use lima_indexer::sync::SyncManager;
use std::collections::HashSet;
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
    /// Largest resumable upload accepted, in bytes (`LIMA_UPLOAD_MAX_BYTES`).
    pub upload_max_bytes: u64,
    /// Serializes the `meta.json` read-modify-write when uploads finish in parallel.
    pub bundle_meta_lock: Arc<tokio::sync::Mutex<()>>,
    /// Upload ids with a PATCH in progress: one writer per upload.
    pub upload_patches: Arc<tokio::sync::Mutex<HashSet<String>>>,
}

/// Behaviour settings, read once from the environment (tests build their own).
#[derive(Debug, Clone)]
pub struct Settings {
    pub trash_days: u64,
    pub collection_sync_hours: u64,
    pub auto_backup: Option<crate::backups::AutoBackup>,
    pub backup_keep: usize,
    pub thingiverse_token: Option<String>,
    /// Watch the library for changes (`LIMA_WATCHER`, on by default).
    pub watcher: bool,
    /// Cap for one resumable upload (`LIMA_UPLOAD_MAX_BYTES`).
    pub upload_max_bytes: u64,
}

impl Settings {
    pub fn from_env() -> Self {
        let auto_backup = crate::backups::AutoBackup::from_env();
        Self {
            trash_days: crate::trash::days_from_env(),
            collection_sync_hours: crate::collection_sync::hours_from_env(),
            backup_keep: auto_backup.map(|a| a.keep).unwrap_or(crate::backups::DEFAULT_KEEP),
            auto_backup,
            thingiverse_token: std::env::var("LIMA_THINGIVERSE_TOKEN").ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            watcher: !matches!(std::env::var("LIMA_WATCHER").unwrap_or_default().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"),
            upload_max_bytes: crate::uploads::max_bytes_from_env(),
        }
    }
}

impl AppState {
    /// The state for a migrated database. Starts nothing in the background.
    pub async fn new(db: Db, paths: Paths, settings: &Settings) -> Result<Self, Box<dyn std::error::Error>> {
        let sync = SyncManager::new(db.pool().clone(), &paths.library, paths.thumbnails()).await?;
        Ok(Self {
            db: Arc::new(db),
            sync,
            paths: Arc::new(paths),
            http: lima_importers::client(),
            imports: Default::default(),
            collection_jobs: Default::default(),
            collection_sync_hours: settings.collection_sync_hours,
            auto_backup: settings.auto_backup,
            backup_keep: settings.backup_keep,
            trash_days: settings.trash_days,
            thingiverse_token: settings.thingiverse_token.clone(),
            upload_max_bytes: settings.upload_max_bytes,
            bundle_meta_lock: Default::default(),
            upload_patches: Default::default(),
        })
    }
}
