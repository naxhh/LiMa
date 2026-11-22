use lima_db::Db;
use lima_indexer::sync::SyncManager;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub sync: SyncManager,
    pub library_dir: PathBuf,
    pub thumbs_dir: PathBuf,
}
