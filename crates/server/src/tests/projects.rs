//! Projects: create, edit, delete to the trash and restore.

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::TestApp;

#[tokio::test]
async fn create_edit_list_and_get() {
    let app = TestApp::new().await;
    assert_eq!(app.get("/api/health").await["db"], true);

    let id = app.project("Articulated Dragon").await;
    assert!(app.library().join("articulated-dragon").is_dir(), "the folder is created");

    let (status, _) = app
        .request(Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "description": "Silk PLA", "tags": ["PLA", "gift"] })))
        .await;
    assert_eq!(status, StatusCode::OK);
    // Saving the same values again is fine, not "not found".
    let (status, body) = app.request(Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "description": "Silk PLA" }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let p = app.get(&format!("/api/projects/{id}")).await;
    assert_eq!(p["name"], "Articulated Dragon");
    assert_eq!(p["description"], "Silk PLA");
    let tags: Vec<&str> = p["tags"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tags.len(), 2);

    let list = app.get("/api/projects?limit=10").await;
    assert_eq!(super::names(&list), vec!["Articulated Dragon"]);

    let (status, body) = app.request(Method::PATCH, "/api/projects/missing", Some(json!({ "name": "x" }))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("project_not_found")));
    let (status, _) = app.request(Method::GET, "/api/projects/missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_goes_to_the_trash_and_restores_with_metadata() {
    let app = TestApp::new().await;
    app.write("benchy/benchy.stl", &super::stl());
    app.sync("incremental").await;
    let id = app.project_in("benchy").await;
    app.request(Method::PATCH, &format!("/api/projects/{id}"), Some(json!({ "name": "Benchy", "tags": ["boat"] }))).await;
    let (status, _) = app.request(Method::PUT, &format!("/api/projects/{id}/favourite"), None).await;
    assert!(status.is_success());

    let (status, body) = app.request(Method::DELETE, &format!("/api/projects/{id}?files=delete"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let trash_id = body["trash_id"].as_str().unwrap().to_string();
    assert!(!app.library().join("benchy").exists(), "folder moved out of the library");
    assert!(app.library().join(".lima-trash").join(&trash_id).join("benchy/benchy.stl").exists());
    assert_eq!(app.get("/api/projects").await["items"].as_array().unwrap().len(), 0);

    let trash = app.get("/api/trash").await;
    assert_eq!(trash["items"][0]["name"], "Benchy");

    let restored = app.post(&format!("/api/trash/{trash_id}/restore"), json!({})).await;
    let p = app.get(&format!("/api/projects/{}", restored["project_id"].as_str().unwrap())).await;
    assert_eq!(p["name"], "Benchy");
    assert_eq!(p["tags"][0]["name"], "boat");
    assert_eq!(p["is_favourite"], true);
    assert_eq!(p["assets"].as_array().unwrap().len(), 1);
    assert_eq!(app.get("/api/trash").await["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn remove_from_lima_keeps_files_and_sync_skips_them() {
    let app = TestApp::new().await;
    app.write("keep/a.stl", &super::stl());
    app.sync("incremental").await;
    let id = app.project_in("keep").await;

    let (status, _) = app.request(Method::DELETE, &format!("/api/projects/{id}?files=keep"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(app.library().join("keep/a.stl").exists());
    app.sync("full").await;
    assert_eq!(app.get("/api/projects").await["items"].as_array().unwrap().len(), 0, "ignored folders stay out");
    assert_eq!(app.get("/api/ignored-folders").await["items"][0]["folder_path"], "keep");
}
