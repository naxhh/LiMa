//! Resumable uploads (tus): from `POST /api/bundles/new` to importing the bundle.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::Engine;
use serde_json::json;

use super::{TestApp, stl};

fn base64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

fn metadata(bundle_id: &str, filename: &str, mtime: u64) -> String {
    format!("bundle_id {},filename {},mtime {}", base64(bundle_id), base64(filename), base64(&mtime.to_string()))
}

/// An empty bundle ready for resumable uploads.
async fn new_bundle(app: &TestApp) -> String {
    let (status, body) = app.request(Method::POST, "/api/bundles/new", None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

/// Starts uploading one file; returns its URL from the Location header.
async fn create_upload(app: &TestApp, bundle_id: &str, filename: &str, length: usize, mtime: u64) -> String {
    let req = Request::post("/api/uploads")
        .header("tus-resumable", "1.0.0")
        .header("upload-length", length)
        .header("upload-metadata", metadata(bundle_id, filename, mtime))
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = app.send_response(req).await;
    assert_eq!(status, StatusCode::CREATED, "{headers:?}");
    assert_eq!(headers["tus-resumable"], "1.0.0");
    headers.get(header::LOCATION).unwrap().to_str().unwrap().to_string()
}

async fn patch(app: &TestApp, url: &str, offset: i64, chunk: &[u8]) -> (StatusCode, axum::http::HeaderMap) {
    let req = Request::patch(url)
        .header("tus-resumable", "1.0.0")
        .header("upload-offset", offset)
        .header(header::CONTENT_TYPE, "application/offset+octet-stream")
        .body(Body::from(chunk.to_vec()))
        .unwrap();
    let (status, headers, _) = app.send_response(req).await;
    (status, headers)
}

async fn head(app: &TestApp, url: &str) -> (StatusCode, axum::http::HeaderMap) {
    let req = Request::head(url).header("tus-resumable", "1.0.0").body(Body::empty()).unwrap();
    let (status, headers, _) = app.send_response(req).await;
    (status, headers)
}

#[tokio::test]
async fn a_large_file_resumes_and_imports() {
    let app = TestApp::new().await;
    let data = stl();
    let half = data.len() / 2;
    let bundle_id = new_bundle(&app).await;

    // Discovery advertises what we implement.
    let req = Request::options("/api/uploads").body(Body::empty()).unwrap();
    let (status, headers, _) = app.send_response(req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(headers["tus-resumable"], "1.0.0");
    assert_eq!(headers["tus-version"], "1.0.0");
    assert_eq!(headers["tus-extension"], "creation,termination");
    assert_eq!(headers["tus-max-size"], crate::uploads::DEFAULT_MAX_BYTES.to_string());

    let url = create_upload(&app, &bundle_id, "model.stl", data.len(), 1_700_000_000_000).await;
    assert!(url.starts_with("/api/uploads/"), "{url}");

    let (status, headers) = patch(&app, &url, 0, &data[..half]).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{headers:?}");
    assert_eq!(headers["upload-offset"], half.to_string());

    let (status, headers) = head(&app, &url).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["upload-offset"], half.to_string());
    assert_eq!(headers["upload-length"], data.len().to_string());
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["tus-resumable"], "1.0.0");

    // A stale offset is refused, the stored bytes are untouched.
    let (status, _) = patch(&app, &url, 0, &data).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, headers) = patch(&app, &url, half as i64, &data[half..]).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{headers:?}");
    let (status, _) = head(&app, &url).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a finished upload is gone");

    // The file is in the bundle's meta.json with its date, size and checksum.
    let bundle_dir = app.state.paths.bundle(&bundle_id).unwrap();
    let meta: lima_domain::models::bundle::BundleMeta = serde_json::from_slice(&std::fs::read(bundle_dir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta.files.len(), 1);
    let file = &meta.files[0];
    assert_eq!(file.name, "model.stl");
    assert_eq!(file.size, data.len() as i64);
    assert_eq!(file.kind, lima_domain::models::asset::asset_kind("model.stl"));
    assert_eq!(file.mime, crate::bundles::guess_mime("model.stl"));

    assert!(file.mtime.as_deref().unwrap().starts_with("2023-11-14T22:13:20"), "{:?}", file.mtime);
    let original = app.state.paths.state.join("original.stl");
    std::fs::write(&original, &data).unwrap();
    assert_eq!(file.checksum.as_deref(), Some(lima_indexer::hash::hash_file(&original).unwrap().as_str()));
    assert_eq!(std::fs::read(bundle_dir.join("model.stl")).unwrap(), data);

    // And the bundle imports into a project like a multipart one.
    let id = app.project("Resumable").await;
    let imported = app.post(&format!("/api/projects/{id}/import"), json!({ "bundle_id": bundle_id })).await;
    assert_eq!(imported["imported"], 1);
    let project = app.get(&format!("/api/projects/{id}")).await;
    assert_eq!(project["assets"][0]["file_path"], "model.stl");
    assert_eq!(std::fs::read(app.library().join("resumable/model.stl")).unwrap(), data);
}

#[tokio::test]
async fn same_name_gets_a_number() {
    let app = TestApp::new().await;
    let bundle_id = new_bundle(&app).await;
    for _ in 0..2 {
        let url = create_upload(&app, &bundle_id, "benchy.stl", 2, 1_700_000_000_000).await;
        let (status, _) = patch(&app, &url, 0, b"hi").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let dir = app.state.paths.bundle(&bundle_id).unwrap();
    let meta: lima_domain::models::bundle::BundleMeta = serde_json::from_slice(&std::fs::read(dir.join("meta.json")).unwrap()).unwrap();
    let mut names: Vec<&str> = meta.files.iter().map(|f| f.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["benchy (2).stl", "benchy.stl"]);
    assert!(dir.join("benchy (2).stl").exists());
}

#[tokio::test]
async fn creation_checks_the_tus_headers() {
    let app = TestApp::new().await;
    let bundle_id = new_bundle(&app).await;

    // No Tus-Resumable.
    let req = Request::post("/api/uploads").header("upload-length", 3).header("upload-metadata", metadata(&bundle_id, "a.stl", 0)).body(Body::empty()).unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::PRECONDITION_FAILED);

    // No Upload-Length.
    let req = Request::post("/api/uploads").header("tus-resumable", "1.0.0").header("upload-metadata", metadata(&bundle_id, "a.stl", 0)).body(Body::empty()).unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::BAD_REQUEST);

    // A name that isn't a plain file name.
    let req = Request::post("/api/uploads")
        .header("tus-resumable", "1.0.0")
        .header("upload-length", 3)
        .header("upload-metadata", metadata(&bundle_id, "../escape.stl", 0))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::BAD_REQUEST);

    // Larger than the server accepts.
    let req = Request::post("/api/uploads")
        .header("tus-resumable", "1.0.0")
        .header("upload-length", crate::uploads::DEFAULT_MAX_BYTES + 1)
        .header("upload-metadata", metadata(&bundle_id, "a.stl", 0))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::PAYLOAD_TOO_LARGE);

    // Unknown bundle.
    let req = Request::post("/api/uploads")
        .header("tus-resumable", "1.0.0")
        .header("upload-length", 3)
        .header("upload-metadata", metadata("00000000-0000-4000-8000-000000000000", "a.stl", 0))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::NOT_FOUND);

    // A chunk must say it's an offset body.
    let url = create_upload(&app, &bundle_id, "a.stl", 3, 0).await;
    let req = Request::patch(&url).header("tus-resumable", "1.0.0").header("upload-offset", 0).body(Body::from(&b"abc"[..])).unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // A body longer than Upload-Length is refused.
    let (status, _) = patch(&app, &url, 0, b"abcd").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, headers) = head(&app, &url).await;
    assert_eq!((status, headers["upload-offset"].to_str().unwrap()), (StatusCode::OK, "0"));
}

#[tokio::test]
async fn terminating_drops_the_partial_bytes() {
    let app = TestApp::new().await;
    let bundle_id = new_bundle(&app).await;
    let url = create_upload(&app, &bundle_id, "a.stl", 3, 0).await;
    patch(&app, &url, 0, b"a").await;

    let req = Request::delete(&url).header("tus-resumable", "1.0.0").body(Body::empty()).unwrap();
    let (status, headers, _) = app.send_response(req).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{headers:?}");
    assert_eq!(headers["tus-resumable"], "1.0.0");
    assert_eq!(head(&app, &url).await.0, StatusCode::NOT_FOUND);

    // The bundle keeps working with a new upload of the same file.
    let again = create_upload(&app, &bundle_id, "a.stl", 3, 0).await;
    assert_eq!(patch(&app, &again, 0, b"abc").await.0, StatusCode::NO_CONTENT);

    let req = Request::delete(&url).header("tus-resumable", "1.0.0").body(Body::empty()).unwrap();
    assert_eq!(app.send_response(req).await.0, StatusCode::NOT_FOUND);
}
