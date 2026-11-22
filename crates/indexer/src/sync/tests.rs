use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lima_db::Db;
use lima_domain::models::sync::SyncRunRow;

use super::*;

struct Fixture {
    dir: PathBuf,
    db: Db,
}

impl Fixture {
    async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("lima-sync-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("library")).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("lima.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        Self { dir, db }
    }

    fn library(&self) -> PathBuf {
        self.dir.join("library")
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.library().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    async fn manager(&self) -> SyncManager {
        SyncManager::new(self.db.pool().clone(), self.library()).await.unwrap()
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sql).fetch_one(self.db.pool()).await.unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

async fn wait_done(pool: &Pool<Sqlite>, run_id: &str) -> SyncRunRow {
    for _ in 0..200 {
        let run = lima_db::queries::sync::get_run(pool, run_id).await.unwrap().unwrap();
        if run.status != "running" {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("sync run {run_id} did not finish");
}

async fn run_full(fx: &Fixture, manager: &SyncManager) -> SyncRunRow {
    let run = manager.start(SyncMode::Full, SyncReason::Manual).await.unwrap();
    let done = wait_done(fx.db.pool(), &run.id).await;
    // The manager clears its slot right after recording the end of the run.
    for _ in 0..40 {
        if manager.current_run_id().await.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    done
}

#[tokio::test(flavor = "multi_thread")]
async fn indexes_existing_folders() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "solid");
    fx.write("benchy/photos/cover.jpg", "jpg");
    fx.write("benchy/.DS_Store", "junk");
    fx.write("dragon/dragon.3mf", "3mf");
    fx.write("loose-file.txt", "not a project");
    fs::create_dir_all(fx.library().join(".hidden-project")).unwrap();

    let manager = fx.manager().await;
    let run = run_full(&fx, &manager).await;

    assert_eq!(run.status, "complete", "error: {}", run.error);
    assert_eq!(run.stats.projects_total, 2);
    assert_eq!(run.stats.projects_added, 2);
    assert_eq!(run.stats.assets_added, 3);
    assert_eq!(fx.count("SELECT COUNT(*) FROM projects").await, 2);
    assert_eq!(fx.count("SELECT COUNT(*) FROM assets").await, 3);
    assert_eq!(
        fx.count("SELECT COUNT(*) FROM assets WHERE file_path = 'photos/cover.jpg' AND kind = 'image'").await,
        1
    );
    // First image becomes the main image.
    assert_eq!(
        fx.count(
            "SELECT COUNT(*) FROM projects p JOIN assets a ON a.id = p.main_image_id WHERE p.folder_path = 'benchy'"
        )
        .await,
        1
    );

    let events = lima_db::queries::sync::list_events(fx.db.pool(), &run.id, 0, 100).await.unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["run_started", "project_added", "project_added", "run_finished"]);

    // A second run with no changes is a no-op.
    let again = run_full(&fx, &manager).await;
    assert_eq!(again.status, "complete");
    assert_eq!(again.stats.projects_added + again.stats.assets_added + again.stats.assets_updated, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn reconciles_changes() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "solid");
    fx.write("benchy/cover.png", "png");
    fx.write("benchy/old.obj", "obj");
    fx.write("dragon/dragon.3mf", "3mf");

    let manager = fx.manager().await;
    assert_eq!(run_full(&fx, &manager).await.status, "complete");

    fx.write("benchy/benchy.stl", "solid but bigger now");
    fx.write("benchy/new.gltf", "gltf");
    fs::remove_file(fx.library().join("benchy/old.obj")).unwrap();
    fs::remove_file(fx.library().join("benchy/cover.png")).unwrap();
    fs::remove_dir_all(fx.library().join("dragon")).unwrap();

    let run = run_full(&fx, &manager).await;
    assert_eq!(run.status, "complete", "error: {}", run.error);
    assert_eq!(run.stats.projects_removed, 1);
    assert_eq!(run.stats.assets_added, 1);
    assert_eq!(run.stats.assets_updated, 1);
    assert_eq!(run.stats.assets_removed, 2);
    assert_eq!(fx.count("SELECT COUNT(*) FROM projects").await, 1);
    assert_eq!(fx.count("SELECT COUNT(*) FROM assets").await, 2);
    // The removed cover was the main image.
    assert_eq!(fx.count("SELECT COUNT(*) FROM projects WHERE main_image_id IS NOT NULL").await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_wipe_when_library_looks_unmounted() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "solid");
    let manager = fx.manager().await;
    assert_eq!(run_full(&fx, &manager).await.status, "complete");

    fs::remove_dir_all(fx.library().join("benchy")).unwrap();
    let run = run_full(&fx, &manager).await;
    assert_eq!(run.status, "failed");
    assert!(run.error.contains("refusing"), "{}", run.error);
    assert_eq!(fx.count("SELECT COUNT(*) FROM projects").await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn fails_when_library_missing() {
    let fx = Fixture::new().await;
    let manager = SyncManager::new(fx.db.pool().clone(), Path::new("/definitely/not/here")).await.unwrap();
    let run = run_full(&fx, &manager).await;
    assert_eq!(run.status, "failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_one_run_at_a_time() {
    let fx = Fixture::new().await;
    for i in 0..50 {
        fx.write(&format!("p{i}/model.stl"), "solid");
    }
    let manager = fx.manager().await;
    let first = manager.start(SyncMode::Full, SyncReason::Manual).await.unwrap();
    match manager.start(SyncMode::Full, SyncReason::Manual).await {
        Err(StartSyncError::AlreadyRunning { run_id }) => assert_eq!(run_id, first.id),
        other => panic!("expected AlreadyRunning, got {other:?}"),
    }
    wait_done(fx.db.pool(), &first.id).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_fails_leftover_runs() {
    let fx = Fixture::new().await;
    lima_db::queries::sync::create_run(fx.db.pool(), "stale", "full", "manual", &now()).await.unwrap();
    let _manager = fx.manager().await;
    let run = lima_db::queries::sync::get_run(fx.db.pool(), "stale").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
}
