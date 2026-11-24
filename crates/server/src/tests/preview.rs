//! The decimated preview mesh endpoint used by the 3D viewer.

use axum::body::Body;
use axum::http::{Request, StatusCode};

use super::{TestApp, stl};

fn header(bytes: &[u8]) -> (usize, usize) {
    assert_eq!(&bytes[..4], b"LMP1");
    let vertices = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let indices = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    (vertices, indices)
}

#[tokio::test]
async fn preview_mesh_is_binary_and_cached() {
    let app = TestApp::new().await;
    app.write("lamp/lamp.stl", &stl());
    app.sync("incremental").await;
    let project = app.project_in("lamp").await;
    let detail = app.get(&format!("/api/projects/{project}")).await;
    let asset = detail["assets"][0]["id"].as_str().unwrap().to_string();

    let (status, bytes) = app
        .send_raw(Request::get(format!("/api/projects/{project}/assets/{asset}/preview")).body(Body::empty()).unwrap())
        .await;
    assert_eq!(status, StatusCode::OK);
    let (vertices, indices) = header(&bytes);
    assert_eq!(vertices, 3, "one triangle");
    assert_eq!(indices, 3);
    assert_eq!(bytes.len(), 12 + vertices * 6 * 4 + indices * 4);

    // A second request reads the cached bytes.
    let (status, again) = app
        .send_raw(Request::get(format!("/api/projects/{project}/assets/{asset}/preview")).body(Body::empty()).unwrap())
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, again);
    assert_eq!(std::fs::read_dir(app.state.paths.preview_meshes()).unwrap().count(), 1);
}

#[tokio::test]
async fn preview_rejects_assets_that_are_not_models() {
    let app = TestApp::new().await;
    app.write("lamp/lamp.stl", &stl());
    app.write("lamp/photo.jpg", b"not really a jpeg");
    app.sync("incremental").await;
    let project = app.project_in("lamp").await;
    let detail = app.get(&format!("/api/projects/{project}")).await;
    let image = detail["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["file_path"] == "photo.jpg")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = app
        .request(axum::http::Method::GET, &format!("/api/projects/{project}/assets/{image}/preview"), None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}
