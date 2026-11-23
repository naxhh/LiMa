//! Uploads: bundles, importing them into a project, zips, and file deletes.

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::{TestApp, stl, zip};

#[tokio::test]
async fn upload_and_import_with_zip_extraction() {
    let app = TestApp::new().await;
    let pack = zip(&[("Pack/parts/body.stl", &stl()), ("Pack/readme.txt", b"hi"), ("../escape.txt", b"no")]);
    let bundle = app.upload(&[("cover.png", b"not really a png"), ("pack.zip", &pack)]).await;
    let id = app.project("Pack").await;

    let res = app.post(&format!("/api/projects/{id}/import"), json!({ "bundle_id": bundle, "extract_zips": true, "keep_zips": true })).await;
    assert_eq!(res["extracted"]["files"], 2);

    let p = app.get(&format!("/api/projects/{id}")).await;
    let mut files: Vec<&str> = p["assets"].as_array().unwrap().iter().map(|a| a["file_path"].as_str().unwrap()).collect();
    files.sort();
    assert_eq!(files, vec!["Pack/parts/body.stl", "Pack/readme.txt", "cover.png", "pack.zip"]);
    assert!(app.library().join("pack/Pack/parts/body.stl").exists());
    assert!(!app.dir.join("escape.txt").exists() && !app.library().join("escape.txt").exists());

    // Zip contents are readable in place.
    let zip_id = p["assets"].as_array().unwrap().iter().find(|a| a["file_path"] == "pack.zip").unwrap()["id"].as_str().unwrap().to_string();
    let contents = app.get(&format!("/api/projects/{id}/assets/{zip_id}/zip")).await;
    assert_eq!(contents["entries"].as_array().unwrap().len(), 2);
    let req = axum::http::Request::get(format!("/api/projects/{id}/assets/{zip_id}/zip/entry?path=Pack%2Fparts%2Fbody.stl")).body(axum::body::Body::empty()).unwrap();
    let (status, bytes) = app.send_raw(req).await;
    assert_eq!((status, bytes), (StatusCode::OK, stl()));

    // The project is found by its new files on the next sync, nothing changes.
    let run = app.sync("incremental").await;
    assert_eq!(run["stats"]["assets_added"], 0);
}

#[tokio::test]
async fn a_broken_zip_fails_the_import_and_leaves_the_bundle() {
    let app = TestApp::new().await;
    let bundle = app.upload(&[("broken.zip", b"PK not a zip")]).await;
    let id = app.project("Broken").await;
    let (status, body) = app.request(Method::POST, &format!("/api/projects/{id}/import"), Some(json!({ "bundle_id": bundle, "extract_zips": true }))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_zip")));
    // Retried without extraction, the zip is imported as is.
    app.post(&format!("/api/projects/{id}/import"), json!({ "bundle_id": bundle })).await;
    assert_eq!(app.get(&format!("/api/projects/{id}")).await["assets"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_a_file_goes_to_the_trash() {
    let app = TestApp::new().await;
    app.write("box/box.stl", &stl());
    app.write("box/photo.jpg", b"jpg");
    app.sync("incremental").await;
    let id = app.project_in("box").await;
    let p = app.get(&format!("/api/projects/{id}")).await;
    let photo = p["assets"].as_array().unwrap().iter().find(|a| a["file_path"] == "photo.jpg").unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(p["main_image_id"], photo.as_str(), "the first image becomes the cover");

    let (status, body) = app.request(Method::DELETE, &format!("/api/projects/{id}/assets/{photo}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let trash_id = body["trash_id"].as_str().unwrap().to_string();
    assert!(!app.library().join("box/photo.jpg").exists());
    assert_eq!(app.get(&format!("/api/projects/{id}")).await["main_image_id"], serde_json::Value::Null);
    assert_eq!(app.get("/api/trash").await["items"][0]["file_path"], "photo.jpg");

    app.post(&format!("/api/trash/{trash_id}/restore"), json!({})).await;
    let p = app.get(&format!("/api/projects/{id}")).await;
    let back = p["assets"].as_array().unwrap().iter().find(|a| a["file_path"] == "photo.jpg").unwrap()["id"].clone();
    assert_eq!(p["main_image_id"], back, "it's the cover again");
}
