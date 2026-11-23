//! The search language through the API, select-all ids and bulk actions.

use axum::http::Method;
use serde_json::json;

use super::{TestApp, names, stl};

async fn library() -> TestApp {
    let app = TestApp::new().await;
    for (folder, file) in [("dragon", "parts/Dragon_Head.stl"), ("bins", "bins.3mf"), ("duct", "duct.step")] {
        app.write(&format!("{folder}/{file}"), &stl());
    }
    app.write("dragon/photo.jpg", b"jpg");
    app.sync("incremental").await;
    let dragon = app.project_in("dragon").await;
    let bins = app.project_in("bins").await;
    app.request(Method::PATCH, &format!("/api/projects/{dragon}"), Some(json!({ "tags": ["Silk PLA", "gift"] }))).await;
    app.request(Method::PATCH, &format!("/api/projects/{bins}"), Some(json!({ "tags": ["Silk PLA"] }))).await;
    app.request(Method::PUT, &format!("/api/projects/{dragon}/favourite"), None).await;
    let room = app.post("/api/collections", json!({ "name": "Living Room" })).await;
    app.post(&format!("/api/collections/{}/projects", room["id"].as_str().unwrap()), json!({ "project_id": bins })).await;
    app
}

async fn search(app: &TestApp, q: &str) -> Vec<String> {
    let mut found = names(&app.get(&format!("/api/projects?limit=50&sort=name&query={}", urlencoding(q))).await);
    found.sort();
    found
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn filters_combine_with_words() {
    let app = library().await;
    assert_eq!(search(&app, r##"#"silk pla""##).await, vec!["bins", "dragon"]);
    assert_eq!(search(&app, r##"#"silk pla" -#gift"##).await, vec!["bins"]);
    assert_eq!(search(&app, r#"in:"living room""#).await, vec!["bins"]);
    assert_eq!(search(&app, "is:fav").await, vec!["dragon"]);
    assert_eq!(search(&app, "-is:fav").await, vec!["bins", "duct"]);
    assert_eq!(search(&app, "has:image").await, vec!["dragon"]);
    assert_eq!(search(&app, "file:head").await, vec!["dragon"]);
    assert_eq!(search(&app, "format:step").await, vec!["duct"], "STEP files are models");
    assert_eq!(search(&app, "dra").await, vec!["dragon"], "word prefixes");
    assert_eq!(search(&app, "dra -is:fav").await, Vec::<String>::new());
}

#[tokio::test]
async fn select_all_ids_and_bulk_tags() {
    let app = library().await;
    let ids = app.get("/api/projects/ids?query=%23%22silk%20pla%22").await;
    let ids: Vec<String> = ids["ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 2);
    let res = app.post("/api/projects/bulk", json!({ "project_ids": ids, "action": "add_tags", "tags": ["printed"] })).await;
    assert_eq!(res["affected"], 2);
    assert_eq!(search(&app, "#printed").await, vec!["bins", "dragon"]);
}
