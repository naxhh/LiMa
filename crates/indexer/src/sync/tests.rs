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

    fn thumbs(&self) -> PathBuf {
        self.dir.join("thumbs")
    }

    fn write_png(&self, rel: &str, width: u32, color: [u8; 3]) {
        let path = self.library().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(width, width / 2, image::Rgb(color)).save(path).unwrap();
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.library().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    async fn manager(&self) -> SyncManager {
        SyncManager::new(self.db.pool().clone(), self.library(), self.thumbs()).await.unwrap()
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
    run_mode(fx, manager, SyncMode::Full).await
}

async fn run_mode(fx: &Fixture, manager: &SyncManager, mode: SyncMode) -> SyncRunRow {
    let run = manager.start(mode, SyncReason::Manual).await.unwrap();
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
    fx.write_png("benchy/photos/cover.png", 64, [10, 20, 30]);
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
        fx.count("SELECT COUNT(*) FROM assets WHERE file_path = 'photos/cover.png' AND kind = 'image'").await,
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
    fx.write_png("benchy/cover.png", 64, [10, 20, 30]);
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
    let manager = SyncManager::new(fx.db.pool().clone(), Path::new("/definitely/not/here"), fx.thumbs()).await.unwrap();
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

async fn stored_hash(fx: &Fixture, file_path: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT file_hash FROM assets WHERE file_path = ?1")
        .bind(file_path)
        .fetch_one(fx.db.pool())
        .await
        .unwrap()
}

fn blake3_hex(contents: &str) -> String {
    blake3::hash(contents.as_bytes()).to_hex().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn hashes_new_and_changed_files() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "solid");
    fx.write("benchy/notes.txt", "notes");

    let manager = fx.manager().await;
    let run = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(run.status, "complete", "error: {}", run.error);
    assert_eq!(run.stats.assets_hashed, 2);
    assert_eq!(stored_hash(&fx, "benchy.stl").await, Some(blake3_hex("solid")));

    // Unchanged files are not re-hashed by an incremental run.
    let again = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(again.stats.assets_hashed, 0);

    // A changed file is re-hashed.
    fx.write("benchy/benchy.stl", "solid v2");
    let changed = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(changed.stats.assets_hashed, 1);
    assert_eq!(changed.stats.assets_updated, 1);
    assert_eq!(stored_hash(&fx, "benchy.stl").await, Some(blake3_hex("solid v2")));

    // A full run verifies every file.
    let full = run_mode(&fx, &manager, SyncMode::Full).await;
    assert_eq!(full.stats.assets_hashed, 2);
    assert_eq!(full.stats.assets_updated, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn backfills_missing_hashes() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "solid");
    let manager = fx.manager().await;
    run_mode(&fx, &manager, SyncMode::Incremental).await;

    sqlx::query("UPDATE assets SET file_hash = NULL").execute(fx.db.pool()).await.unwrap();
    let run = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(run.stats.assets_hashed, 1);
    assert_eq!(run.stats.assets_updated, 0);
    assert_eq!(stored_hash(&fx, "benchy.stl").await, Some(blake3_hex("solid")));
}

#[tokio::test(flavor = "multi_thread")]
async fn full_run_detects_in_place_rewrites() {
    let fx = Fixture::new().await;
    fx.write("benchy/benchy.stl", "aaaaa");
    let manager = fx.manager().await;
    run_mode(&fx, &manager, SyncMode::Incremental).await;

    // Same size, same mtime, different bytes.
    let path = fx.library().join("benchy/benchy.stl");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "bbbbb").unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();

    let incremental = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(incremental.stats.assets_updated, 0, "incremental trusts stat data");

    let full = run_mode(&fx, &manager, SyncMode::Full).await;
    assert_eq!(full.stats.assets_updated, 1);
    assert_eq!(stored_hash(&fx, "benchy.stl").await, Some(blake3_hex("bbbbb")));
    let events = lima_db::queries::sync::list_events(fx.db.pool(), &full.id, 0, 100).await.unwrap();
    assert!(events.iter().any(|e| e.kind == "asset_content_changed"));
}

async fn asset_id(fx: &Fixture, file_path: &str) -> (String, String) {
    sqlx::query_as::<_, (String, String)>("SELECT project_id, id FROM assets WHERE file_path = ?1")
        .bind(file_path)
        .fetch_one(fx.db.pool())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn manages_thumbnails() {
    let fx = Fixture::new().await;
    fx.write_png("benchy/cover.png", 1200, [200, 0, 0]);
    fx.write("benchy/broken.jpg", "not really a jpeg");
    fx.write("benchy/benchy.stl", "solid");
    fx.write_png("dragon/dragon.png", 100, [0, 200, 0]);

    let manager = fx.manager().await;
    let run = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(run.status, "complete", "error: {}", run.error);
    assert_eq!(run.stats.thumbnails_generated, 2);
    assert_eq!(run.stats.errors, 1, "broken image is reported");

    let (project_id, cover_id) = asset_id(&fx, "cover.png").await;
    let thumb = crate::thumbnails::thumb_path(&fx.thumbs(), &project_id, &cover_id);
    let img = image::open(&thumb).unwrap();
    assert_eq!((img.width(), img.height()), (512, 256));

    let events = lima_db::queries::sync::list_events(fx.db.pool(), &run.id, 0, 100).await.unwrap();
    assert!(events.iter().any(|e| e.kind == "thumbnail_failed" && e.data["path"] == "benchy/broken.jpg"));

    // Unchanged images keep their thumbnail; a deleted thumbnail is recreated.
    let (dragon_project, dragon_id) = asset_id(&fx, "dragon.png").await;
    let dragon_thumb = crate::thumbnails::thumb_path(&fx.thumbs(), &dragon_project, &dragon_id);
    fs::remove_file(&dragon_thumb).unwrap();
    let again = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(again.stats.thumbnails_generated, 1);
    assert!(dragon_thumb.exists());

    // A changed image gets a fresh thumbnail.
    fx.write_png("benchy/cover.png", 300, [0, 0, 200]);
    let changed = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(changed.stats.thumbnails_generated, 1);
    assert_eq!(image::open(&thumb).unwrap().width(), 300);

    // Removed assets and projects drop their thumbnails.
    fs::remove_file(fx.library().join("benchy/cover.png")).unwrap();
    fs::remove_dir_all(fx.library().join("dragon")).unwrap();
    let removed = run_mode(&fx, &manager, SyncMode::Incremental).await;
    assert_eq!(removed.status, "complete");
    assert!(!thumb.exists());
    assert!(!fx.thumbs().join(&dragon_project).exists());
}
