//! Library-wide features: duplicates and stored backups.

use serde_json::json;

use super::{TestApp, names, stl};

#[tokio::test]
async fn duplicates_keep_one_and_merge() {
    let app = TestApp::new().await;
    app.write("benchy/benchy.stl", &stl());
    app.write("benchy-copy/benchy.stl", &stl());
    app.write("other/other.stl", b"different");
    app.sync("incremental").await;
    let keep = app.project_in("benchy").await;
    let copy = app.project_in("benchy-copy").await;
    app.request(axum::http::Method::PATCH, &format!("/api/projects/{copy}"), Some(json!({ "tags": ["boat"] }))).await;

    let dups = app.get("/api/duplicates").await;
    assert_eq!(dups["project_groups"].as_array().unwrap().len(), 1);

    let res = app.post("/api/duplicates/resolve", json!({ "keep_id": keep, "remove_ids": [copy], "remove": "keep_files" })).await;
    assert_eq!((res["merged"]["tags"].as_i64(), res["removed"].as_i64()), (Some(1), Some(1)));
    let mut left = names(&app.get("/api/projects").await);
    left.sort();
    assert_eq!(left, vec!["benchy", "other"]);
    assert_eq!(app.get(&format!("/api/projects/{keep}")).await["tags"][0]["name"], "boat");
    assert!(app.library().join("benchy-copy/benchy.stl").exists(), "files kept");
    assert_eq!(app.get("/api/duplicates").await["project_groups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn stored_backups_restore() {
    let app = TestApp::new().await;
    app.write("lamp/lamp.stl", &stl());
    app.sync("incremental").await;
    let id = app.project_in("lamp").await;
    app.request(axum::http::Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "name": "Moon lamp" }))).await;

    let file = app.post("/api/metadata/backups", json!({})).await;
    let name = file["name"].as_str().unwrap().to_string();
    assert_eq!(app.get("/api/metadata/backups").await["items"].as_array().unwrap().len(), 1);

    app.request(axum::http::Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "name": "Renamed" }))).await;
    let dry = app.post(&format!("/api/metadata/backups/{name}/restore?dry_run=true"), json!({})).await;
    assert_eq!(dry["dry_run"], true);
    assert_eq!(app.get(&format!("/api/projects/{id}")).await["name"], "Renamed", "a dry run writes nothing");
    app.post(&format!("/api/metadata/backups/{name}/restore"), json!({})).await;
    assert_eq!(app.get(&format!("/api/projects/{id}")).await["name"], "Moon lamp");
}
