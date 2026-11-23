//! Tag cleanup endpoints: merging one tag into another and bulk delete.

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::TestApp;

async fn tag_id(app: &TestApp, name: &str) -> String {
    let list = app.get("/api/tags?limit=200").await;
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn tag_names(project: &serde_json::Value) -> Vec<String> {
    project["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn merge_and_bulk_delete_tags() {
    let app = TestApp::new().await;
    let dragon = app.project("Dragon").await;
    let benchy = app.project("Benchy").await;
    app.request(Method::PATCH, &format!("/api/projects/{dragon}"), Some(json!({ "tags": ["boats", "gift"] }))).await;
    app.request(Method::PATCH, &format!("/api/projects/{benchy}"), Some(json!({ "tags": ["boats"] }))).await;

    let boats = tag_id(&app, "boats").await;
    let gift = tag_id(&app, "gift").await;

    // Merge "boats" into "gift": both projects gain "gift", "boats" disappears.
    let (status, body) = app
        .request(Method::POST, &format!("/api/tags/{boats}/merge"), Some(json!({ "into_tag_id": gift })))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["projects_affected"], 2);

    assert_eq!(tag_names(&app.get(&format!("/api/projects/{dragon}")).await), vec!["gift"]);
    assert_eq!(tag_names(&app.get(&format!("/api/projects/{benchy}")).await), vec!["gift"]);

    let tags = app.get("/api/tags?limit=200").await;
    assert!(!tags["items"].as_array().unwrap().iter().any(|t| t["name"] == "boats"), "merged-away tag is gone");

    // Bulk delete the remaining tag (plus one that doesn't exist).
    let (status, body) = app
        .request(Method::POST, "/api/tags/delete", Some(json!({ "tag_ids": [gift, "missing"] })))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], 1);
    assert_eq!(app.get("/api/tags?limit=200").await["items"].as_array().unwrap().len(), 0);

    // Same tag into itself is rejected; a missing tag is a 404.
    let (status, _) = app.request(Method::POST, "/api/tags/x/merge", Some(json!({ "into_tag_id": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .request(Method::POST, "/api/tags/missing/merge", Some(json!({ "into_tag_id": "also-missing" })))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
