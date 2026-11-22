//! MakerWorld, through the Bambu design-service API. Metadata is public;
//! downloads need a Bambu access token (see [`crate::bambu`]).
//!
//! `api.bambulab.com` serves the same API as `makerworld.com/api` without the
//! Cloudflare challenge in front of the website.

use serde::Deserialize;

use crate::{ImportError, NameSet, RemoteFile, RemoteImage, RemoteModel, Result, Source, html_to_text};

const API: &str = "https://api.bambulab.com/v1/design-service";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Design {
    id: i64,
    title: String,
    slug: String,
    summary: String,
    cover_url: String,
    tags: Option<Vec<String>>,
    license: String,
    design_creator: Option<Creator>,
    design_extension: Option<Extension>,
    instances: Option<Vec<Instance>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Creator {
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Extension {
    design_pictures: Option<Vec<Picture>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Picture {
    name: String,
    url: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Instance {
    id: i64,
    title: String,
    is_default: bool,
    extention: Option<InstanceExtension>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InstanceExtension {
    model_info: Option<ModelInfo>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ModelInfo {
    f3mf: Option<FileInfo>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FileInfo {
    size: Option<i64>,
}

/// Body of an error answer, e.g. `{"code":1,"error":"Please log in to download models."}`.
#[derive(Deserialize)]
struct ApiError {
    error: Option<String>,
}

/// Successful answer of the download endpoint.
#[derive(Deserialize)]
struct DownloadInfo {
    url: String,
}

fn upstream(message: String) -> ImportError {
    ImportError::Upstream { site: "makerworld", message }
}

pub async fn fetch(client: &reqwest::Client, id: &str) -> Result<RemoteModel> {
    let res = client.get(format!("{API}/design/{id}")).header("accept", "application/json").send().await?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ImportError::NotFound);
    }
    if !status.is_success() {
        return Err(upstream(format!("{status}: {}", res.text().await.unwrap_or_default())));
    }
    let design: Design = res.json().await.map_err(|e| upstream(e.to_string()))?;
    // Unknown ids answer 200 with an all-empty design.
    if design.id == 0 {
        return Err(ImportError::NotFound);
    }
    Ok(to_model(design))
}

pub(crate) fn to_model(d: Design) -> RemoteModel {
    let mut names = NameSet::default();

    let mut pictures = d.design_extension.and_then(|e| e.design_pictures).unwrap_or_default();
    if pictures.is_empty() && !d.cover_url.is_empty() {
        pictures.push(Picture { name: String::new(), url: d.cover_url.clone() });
    }
    let images = pictures
        .into_iter()
        .filter(|p| !p.url.is_empty())
        .map(|p| {
            let name = if p.name.trim().is_empty() {
                p.url.rsplit('/').next().unwrap_or("image.png").to_string()
            } else {
                p.name
            };
            RemoteImage { url: p.url, name: names.claim(&name) }
        })
        .collect();

    // Each instance is a print profile with its own sliced 3MF. Default first.
    let mut instances = d.instances.unwrap_or_default();
    instances.sort_by_key(|i| !i.is_default);
    let files = instances
        .into_iter()
        .map(|i| {
            let title = if i.title.trim().is_empty() { format!("profile-{}", i.id) } else { i.title.trim().to_string() };
            RemoteFile {
                id: i.id.to_string(),
                name: names.claim(&format!("{title}.3mf")),
                size: i.extention.and_then(|e| e.model_info).and_then(|m| m.f3mf).and_then(|f| f.size).filter(|s| *s > 0),
                kind: "model".to_string(),
                group: Some(if i.is_default { "Print profiles (default first)".to_string() } else { "Print profiles".to_string() }),
            }
        })
        .collect();

    let slug = if d.slug.is_empty() { String::new() } else { format!("-{}", d.slug) };
    RemoteModel {
        source: Source::Makerworld,
        source_id: d.id.to_string(),
        url: format!("https://makerworld.com/en/models/{}{slug}", d.id),
        name: d.title.trim().to_string(),
        description: html_to_text(&d.summary),
        author: d.design_creator.map(|c| c.name).filter(|n| !n.is_empty()),
        license: Some(d.license).filter(|l| !l.is_empty()),
        tags: d.tags.unwrap_or_default(),
        images,
        files,
    }
}

/// Resolves the signed download URL of one print profile's 3MF.
pub async fn download_url(client: &reqwest::Client, file: &RemoteFile, token: &str) -> Result<String> {
    if !file.id.chars().all(|c| c.is_ascii_digit()) {
        return Err(ImportError::UnknownFile(file.id.clone()));
    }
    let res = client
        .get(format!("{API}/instance/{}/f3mf?type=download", file.id))
        .bearer_auth(token)
        .header("accept", "application/json")
        .send()
        .await?;
    let status = res.status();
    let body = res.text().await?;
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        let message = serde_json::from_str::<ApiError>(&body).ok().and_then(|e| e.error).unwrap_or_default();
        tracing::warn!("MakerWorld refused download of {}: {status} {message}", file.id);
        // A valid token can still be refused, e.g. for paid models; only a login
        // message means the token itself is the problem.
        if message.to_ascii_lowercase().contains("log in") || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ImportError::LoginRequired);
        }
        return Err(upstream(format!("{status}: {message}")));
    }
    if !status.is_success() {
        return Err(upstream(format!("{status}: {body}")));
    }
    let info: DownloadInfo =
        serde_json::from_str(&body).map_err(|e| upstream(format!("unexpected download answer ({e}): {}", truncate(&body))))?;
    Ok(info.url)
}

fn truncate(s: &str) -> String {
    s.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_design() {
        let design: Design = serde_json::from_value(json!({
            "id": 1400373, "title": "Seed Starter ", "slug": "seed-starter",
            "summary": "<h2>Kit</h2><p>Self watering</p>",
            "coverUrl": "https://cdn/cover.png",
            "tags": ["seed", "planter"], "license": "Standard Digital File License",
            "designCreator": { "name": "meyui", "uid": 1 },
            "designExtension": { "design_pictures": [
                { "name": "3.png", "url": "https://cdn/a.png" },
                { "name": "3.png", "url": "https://cdn/b.png" }
            ] },
            "instances": [
                { "id": 11, "title": "9 cells", "isDefault": false, "extention": { "modelInfo": { "f3mf": { "size": 2870647 } } } },
                { "id": 12, "title": "Full kit", "isDefault": true, "extention": { "modelInfo": { "f3mf": { "size": 0 } } } }
            ],
            "unknownField": true
        }))
        .unwrap();

        let m = to_model(design);
        assert_eq!(m.name, "Seed Starter");
        assert_eq!(m.url, "https://makerworld.com/en/models/1400373-seed-starter");
        assert_eq!(m.author.as_deref(), Some("meyui"));
        assert!(m.description.contains("Kit") && m.description.contains("Self watering"));
        let images: Vec<&str> = m.images.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(images, vec!["3.png", "3-2.png"]);
        let files: Vec<(&str, &str, Option<i64>)> = m.files.iter().map(|f| (f.id.as_str(), f.name.as_str(), f.size)).collect();
        assert_eq!(files, vec![("12", "Full kit.3mf", None), ("11", "9 cells.3mf", Some(2870647))]);
    }

    #[test]
    fn falls_back_to_cover() {
        let design: Design = serde_json::from_value(json!({ "id": 5, "title": "x", "coverUrl": "https://cdn/m/cover.png" })).unwrap();
        let m = to_model(design);
        assert_eq!(m.images.len(), 1);
        assert_eq!(m.images[0].name, "cover.png");
    }
}
