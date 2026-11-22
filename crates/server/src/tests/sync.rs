//! Sync: the library folder is the source of truth.

use axum::http::Method;
use serde_json::json;

use super::{TestApp, names, stl};

#[tokio::test]
async fn sync_indexes_folders_and_is_idempotent() {
    let app = TestApp::new().await;
    app.write("benchy/benchy.stl", &stl());
    app.write("benchy/photo.jpg", b"jpg");
    app.write("bins/bins.3mf", b"3mf");

    let first = app.sync("incremental").await;
    assert_eq!(first["status"], "complete");
    assert_eq!((first["stats"]["projects_added"].as_i64(), first["stats"]["assets_added"].as_i64()), (Some(2), Some(3)));
    let mut found = names(&app.get("/api/projects?sort=name").await);
    found.sort();
    assert_eq!(found, vec!["benchy", "bins"]);
    let p = app.get(&format!("/api/projects/{}", app.project_in("benchy").await)).await;
    assert!(p["assets"].as_array().unwrap().iter().all(|a| a["file_hash"].is_string()), "every file is hashed");

    // Running again changes nothing, incremental or full.
    for mode in ["incremental", "full"] {
        let again = app.sync(mode).await;
        let s = &again["stats"];
        assert_eq!(
            (s["projects_added"].as_i64(), s["projects_removed"].as_i64(), s["assets_added"].as_i64(), s["assets_updated"].as_i64(), s["assets_removed"].as_i64()),
            (Some(0), Some(0), Some(0), Some(0), Some(0)),
            "{mode} sync of an unchanged library: {s}"
        );
    }

    // A changed file, a new file, a removed folder.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    app.write("benchy/benchy.stl", b"changed");
    app.write("benchy/notes.txt", b"n");
    std::fs::remove_dir_all(app.library().join("bins")).unwrap();
    let s = app.sync("incremental").await["stats"].clone();
    assert_eq!((s["assets_updated"].as_i64(), s["assets_added"].as_i64(), s["projects_removed"].as_i64()), (Some(1), Some(1), Some(1)), "{s}");
    assert_eq!(names(&app.get("/api/projects").await), vec!["benchy"]);
}

#[tokio::test]
async fn rebuild_from_disk_then_restore_metadata() {
    let app = TestApp::new().await;
    app.write("dragon/dragon.stl", &stl());
    app.write("dragon/photo.jpg", b"jpg");
    app.sync("incremental").await;
    let id = app.project_in("dragon").await;
    app.request(Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "name": "Articulated dragon", "description": "Silk", "tags": ["PLA"] }))).await;
    let desk = app.post("/api/collections", json!({ "name": "Desk" })).await;
    app.post(&format!("/api/collections/{}/projects", desk["id"].as_str().unwrap()), json!({ "project_id": id })).await;
    let backup = app.get("/api/metadata/export").await;

    // The database is lost; the library folder survives.
    let app = app.rebuilt().await;
    app.sync("incremental").await;
    assert_eq!(names(&app.get("/api/projects").await), vec!["dragon"], "rebuilt from disk, folder names only");

    let report = app.post("/api/metadata/restore", backup).await;
    assert_eq!(report["matched_by_folder"], 1);
    let p = app.get(&format!("/api/projects/{}", app.project_in("dragon").await)).await;
    assert_eq!(p["name"], "Articulated dragon");
    assert_eq!(p["description"], "Silk");
    assert_eq!(p["tags"][0]["name"], "PLA");
    assert_eq!(p["collections"][0]["name"], "Desk");
    assert!(p["main_image_id"].is_string(), "the cover comes back");
}
