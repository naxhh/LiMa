//! Thingiverse, through its REST API (`api.thingiverse.com`). Every call needs an app
//! token: anyone can create one at <https://www.thingiverse.com/apps/create>, and it
//! reads public things. Downloads go through the API too; the website refuses scripts.
//!
//! Cloudflare sits in front and challenges clients that claim to be a browser but don't
//! look like one, so these requests say who they are instead of reusing the
//! browser-like user agent.

use reqwest::header::{AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

use crate::{ImportError, NameSet, RemoteFile, RemoteImage, RemoteModel, Result, Source, file_kind, html_to_markdown};

const API: &str = "https://api.thingiverse.com";
const AGENT: &str = concat!("LiMa/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize)]
pub(crate) struct Thing {
    id: i64,
    name: String,
    public_url: Option<String>,
    creator: Option<Creator>,
    /// HTML; preferred over the plain `description`.
    description_html: Option<String>,
    description: Option<String>,
    license: Option<String>,
    #[serde(default)]
    tags: Vec<Tag>,
}

#[derive(Deserialize)]
struct Creator {
    name: Option<String>,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
}

#[derive(Deserialize)]
pub(crate) struct File {
    id: i64,
    name: String,
    size: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct Image {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    sizes: Vec<ImageSize>,
}

#[derive(Deserialize)]
struct ImageSize {
    #[serde(rename = "type")]
    kind: String,
    size: String,
    url: String,
}

fn upstream(message: String) -> ImportError {
    ImportError::Upstream { site: "thingiverse", message }
}

async fn get<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, token: &str, path: &str) -> Result<T> {
    let res = client.get(format!("{API}{path}")).header(USER_AGENT, AGENT).header(AUTHORIZATION, format!("Bearer {token}")).send().await?;
    let status = res.status();
    match status.as_u16() {
        200..=299 => res.json().await.map_err(|e| upstream(format!("{path}: {e}"))),
        404 => Err(ImportError::NotFound),
        401 | 403 if is_challenge(&res) => Err(upstream("Cloudflare blocked the request; try again later".into())),
        401 | 403 => Err(ImportError::TokenRejected),
        _ => Err(upstream(format!("{path} returned {status}"))),
    }
}

/// Cloudflare's bot check answers with an HTML page instead of the API's JSON.
fn is_challenge(res: &reqwest::Response) -> bool {
    res.headers().contains_key("cf-mitigated")
        || res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"))
}

pub async fn fetch(client: &reqwest::Client, id: &str, token: Option<&str>) -> Result<RemoteModel> {
    let token = token.ok_or(ImportError::TokenMissing)?;
    let thing: Thing = get(client, token, &format!("/things/{id}")).await?;
    let files: Vec<File> = get(client, token, &format!("/things/{id}/files")).await?;
    let images: Vec<Image> = get(client, token, &format!("/things/{id}/images")).await.unwrap_or_default();
    Ok(to_model(thing, files, images))
}

/// The original picture (on the CDN; the API lists it under the "thumb" sizes), else the
/// biggest resized one. The image's own `url` is an API address, not a picture.
fn best_image_url(img: &Image) -> Option<String> {
    const ORDER: [&str; 4] = ["large", "featured", "medium", "small"];
    img.sizes
        .iter()
        .find(|s| s.url.starts_with("https://cdn.thingiverse.com/"))
        .or_else(|| ORDER.iter().find_map(|want| img.sizes.iter().find(|s| s.kind == "display" && s.size == *want)))
        .map(|s| s.url.clone())
}

/// Images are served behind the same Cloudflare check as the API.
pub async fn download_image(client: &reqwest::Client, url: &str, dest: &std::path::Path) -> Result<std::path::PathBuf> {
    crate::download_with(client.get(url).header(USER_AGENT, AGENT), dest).await
}

pub(crate) fn to_model(t: Thing, files: Vec<File>, images: Vec<Image>) -> RemoteModel {
    let mut names = NameSet::default();
    let images = images
        .iter()
        .filter_map(|img| {
            let url = best_image_url(img)?;
            let fallback = url.rsplit('/').next().unwrap_or("image.jpg").split('?').next().unwrap_or("image.jpg").to_string();
            let name = img.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or(fallback);
            Some(RemoteImage { url, name: names.claim(&name) })
        })
        .collect();
    let files = files
        .into_iter()
        .map(|f| RemoteFile {
            id: f.id.to_string(),
            kind: file_kind(&f.name).to_string(),
            name: names.claim(&f.name),
            size: f.size,
            group: None,
        })
        .collect();

    let mut description = html_to_markdown(t.description_html.as_deref().unwrap_or(""));
    if description.is_empty() {
        description = t.description.unwrap_or_default().trim().to_string();
    }
    RemoteModel {
        source: Source::Thingiverse,
        url: t.public_url.unwrap_or_else(|| format!("https://www.thingiverse.com/thing:{}", t.id)),
        source_id: t.id.to_string(),
        name: t.name.trim().to_string(),
        description,
        author: t.creator.and_then(|c| c.name),
        license: t.license.filter(|l| !l.trim().is_empty()),
        tags: t.tags.into_iter().map(|t| t.name).collect(),
        images,
        files,
    }
}

/// Where one file downloads from: the API answers with a redirect to the file, which is
/// followed with the same token.
pub async fn download(client: &reqwest::Client, file: &RemoteFile, token: &str, dest: &std::path::Path) -> Result<std::path::PathBuf> {
    let url = format!("{API}/files/{}/download", file.id);
    crate::download_with(client.get(url).header(USER_AGENT, AGENT).header(AUTHORIZATION, format!("Bearer {token}")), dest).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_thing() {
        let thing: Thing = serde_json::from_value(json!({
            "id": 763622, "name": " #3DBenchy ", "public_url": "https://www.thingiverse.com/thing:763622",
            "creator": { "name": "CreativeTools" },
            "description": "plain", "description_html": "<p>The <b>jolly</b> torture test</p>",
            "license": "Public Domain", "tags": [{ "name": "benchy" }, { "name": "boat" }]
        }))
        .unwrap();
        let files: Vec<File> = serde_json::from_value(json!([
            { "id": 1, "name": "3DBenchy.stl", "size": 11285384 },
            { "id": 2, "name": "3DBenchy.stl", "size": 11285384 },
            { "id": 3, "name": "readme.txt" }
        ]))
        .unwrap();
        let images: Vec<Image> = serde_json::from_value(json!([
            { "name": "benchy.jpg", "url": "https://api.thingiverse.com/things/763622/images/1", "sizes": [
                { "type": "preview", "size": "large", "url": "https://resize.thingiverse.com/?url=x" },
                { "type": "thumb", "size": "large", "url": "https://cdn.thingiverse.com/assets/ee/benchy.jpg" }
            ]},
            { "url": "https://api.thingiverse.com/things/763622/images/2", "sizes": [
                { "type": "display", "size": "medium", "url": "https://resize.thingiverse.com/m/side.png" },
                { "type": "display", "size": "large", "url": "https://resize.thingiverse.com/l/side.png" }
            ]},
            { "sizes": [] }
        ]))
        .unwrap();

        let m = to_model(thing, files, images);
        assert_eq!(m.name, "#3DBenchy");
        assert_eq!(m.source_id, "763622");
        assert_eq!(m.author.as_deref(), Some("CreativeTools"));
        assert_eq!(m.description, "The **jolly** torture test");
        assert_eq!(m.tags, vec!["benchy", "boat"]);
        assert_eq!(m.images.len(), 2, "images without a picture are skipped");
        assert_eq!(m.images[0].url, "https://cdn.thingiverse.com/assets/ee/benchy.jpg", "the original");
        assert_eq!(m.images[1].url, "https://resize.thingiverse.com/l/side.png", "else the biggest resized one");
        assert_eq!(m.images[1].name, "side.png");
        let files: Vec<(&str, &str, &str)> = m.files.iter().map(|f| (f.id.as_str(), f.name.as_str(), f.kind.as_str())).collect();
        assert_eq!(files, vec![("1", "3DBenchy.stl", "model"), ("2", "3DBenchy-2.stl", "model"), ("3", "readme.txt", "other")]);
    }
}
