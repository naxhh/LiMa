//! API tests: the real router, called in-process, over a temporary library and database.
//! Nothing runs in the background (no watcher, backups or schedulers).

mod library;
mod projects;
mod search;
mod sync;
mod tags;
mod uploads;

use std::path::{Path, PathBuf};

use axum::{Router, body::Body, http::{Method, Request, StatusCode}};
use serde_json::Value;
use tower::ServiceExt;

use crate::state::{AppState, Settings};

pub struct TestApp {
    pub state: AppState,
    router: Router,
    dir: PathBuf,
    /// Keeps the folder when another app reuses it (see `rebuilt`).
    keep_dir: bool,
}

impl TestApp {
    pub async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("lima-api-test-{}", uuid::Uuid::new_v4()));
        Self::at(dir).await
    }

    /// An app over `dir/library` with its own state in `dir/<random>`.
    async fn at(dir: PathBuf) -> Self {
        let state_dir = dir.join(format!("state-{}", uuid::Uuid::new_v4()));
        let lookup = |key: &str| match key {
            "LIMA_LIBRARY_DIR" => Some(dir.join("library").display().to_string()),
            "LIMA_STATE_DIR" => Some(state_dir.display().to_string()),
            "LIMA_UI_DIR" => Some(dir.join("ui").display().to_string()),
            _ => None,
        };
        let paths = lima_domain::config::Paths::from_lookup(lookup);
        for d in paths.required_dirs() {
            std::fs::create_dir_all(d).unwrap();
        }
        let db = lima_db::Db::connect(&paths.database_url).await.unwrap();
        db.migrate().await.unwrap();
        lima_db::queries::collections::ensure_favourites(db.pool(), &crate::backups::now()).await.unwrap();
        let settings = Settings { trash_days: 30, collection_sync_hours: 0, auto_backup: None, backup_keep: 7, thingiverse_token: None, watcher: false };
        let state = AppState::new(db, paths, &settings).await.unwrap();
        Self { router: crate::app::router(state.clone()), state, dir, keep_dir: false }
    }

    /// A fresh database over the same library, as after losing the database.
    pub async fn rebuilt(mut self) -> Self {
        self.keep_dir = true;
        Self::at(self.dir.clone()).await
    }

    pub fn library(&self) -> PathBuf {
        self.dir.join("library")
    }

    /// Writes a file into the library (creating folders).
    pub fn write(&self, rel: &str, contents: &[u8]) {
        let path = self.library().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub async fn request(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header("content-type", "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        self.send(req.body(body).unwrap()).await
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let (status, bytes) = self.send_raw(req).await;
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    pub async fn send_raw(&self, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, bytes.to_vec())
    }

    pub async fn get(&self, uri: &str) -> Value {
        let (status, json) = self.request(Method::GET, uri, None).await;
        assert!(status.is_success(), "GET {uri}: {status} {json}");
        json
    }

    pub async fn post(&self, uri: &str, body: Value) -> Value {
        let (status, json) = self.request(Method::POST, uri, Some(body)).await;
        assert!(status.is_success(), "POST {uri}: {status} {json}");
        json
    }

    /// Uploads files as a bundle (multipart, like the browser) and returns its id.
    pub async fn upload(&self, files: &[(&str, &[u8])]) -> String {
        let boundary = "lima-test-boundary";
        let mut body = Vec::new();
        for (name, data) in files {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files[]\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let req = Request::post("/api/bundles").header("content-type", format!("multipart/form-data; boundary={boundary}")).body(Body::from(body)).unwrap();
        let (status, json) = self.send(req).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        json["id"].as_str().unwrap().to_string()
    }

    /// Runs a sync and waits for it. Returns the run (with its stats).
    pub async fn sync(&self, mode: &str) -> Value {
        let run = self.post("/api/sync/run", serde_json::json!({ "mode": mode })).await;
        self.state.sync.wait_idle().await;
        self.get(&format!("/api/sync/runs/{}", run["id"].as_str().unwrap())).await
    }

    /// A new empty project; returns its id.
    pub async fn project(&self, name: &str) -> String {
        self.post("/api/projects", serde_json::json!({ "name": name })).await["id"].as_str().unwrap().to_string()
    }

    /// `(id, folder)` of the project in `folder`.
    pub async fn project_in(&self, folder: &str) -> String {
        sqlx::query_scalar("SELECT id FROM projects WHERE folder_path = ?1").bind(folder).fetch_one(self.state.db.pool()).await.unwrap()
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if !self.keep_dir {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Names of the projects a list response holds, in order.
pub fn names(list: &Value) -> Vec<String> {
    list["items"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect()
}

/// A small valid binary STL (one triangle) so model handling has real input.
pub fn stl() -> Vec<u8> {
    let mut b = vec![0u8; 80];
    b.extend_from_slice(&1u32.to_le_bytes());
    for v in [0f32, 0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 0.] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&[0, 0]);
    b
}

pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[allow(dead_code)]
pub fn exists(p: &Path) -> bool {
    p.exists()
}
