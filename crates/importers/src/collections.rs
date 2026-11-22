//! Collections on external sites: lists of models that can be imported (and kept in sync)
//! together. Supported: MakerWorld (public, no login) and Thingiverse (app token).
//! Printables doesn't expose the models of a collection in its public API yet.

use reqwest::header::{AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

use crate::{ImportError, RemoteFile, RemoteModel, Result, Source, Tokens, html_to_markdown};

/// Most models read from one collection.
pub const MAX_MODELS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRef {
    pub source: Source,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteCollection {
    pub source: Source,
    pub source_id: String,
    pub url: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    /// In the site's order.
    pub models: Vec<CollectionModel>,
    /// More models than `MAX_MODELS`; the rest were left out.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionModel {
    /// The model's id on the site (what [`crate::ModelRef`] takes).
    pub id: String,
    pub name: String,
    /// Canonical model page URL, as stored in a project's `source_url`.
    pub url: String,
    pub cover: Option<String>,
    pub author: Option<String>,
}

/// Recognises a MakerWorld or Thingiverse collection URL.
pub fn parse_collection_url(raw: &str) -> Option<CollectionRef> {
    let url = url::Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.trim_start_matches("www.").to_ascii_lowercase();
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let source = match host.as_str() {
        "makerworld.com" => Source::Makerworld,
        "thingiverse.com" => Source::Thingiverse,
        _ => return None,
    };
    // MakerWorld: "/<lang>/collections/<id>-<slug>"; Thingiverse: "/<user>/collections/<id>".
    let pos = segments.iter().position(|s| *s == "collections")?;
    let id: String = segments.get(pos + 1)?.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(CollectionRef { source, id })
}

pub async fn fetch_collection(client: &reqwest::Client, c: &CollectionRef, tokens: Tokens<'_>) -> Result<RemoteCollection> {
    match c.source {
        Source::Makerworld => makerworld(client, &c.id).await,
        Source::Thingiverse => thingiverse(client, &c.id, tokens.thingiverse.ok_or(ImportError::TokenMissing)?).await,
        Source::Printables => Err(ImportError::UnsupportedUrl),
    }
}

/// The files a collection import downloads for each model: MakerWorld's default print
/// profile, else every printable model file (everything when there's none).
pub fn default_files(model: &RemoteModel) -> Vec<RemoteFile> {
    if model.source == Source::Makerworld {
        return model.files.iter().take(1).cloned().collect();
    }
    let models: Vec<RemoteFile> = model.files.iter().filter(|f| f.kind == "model").cloned().collect();
    if models.is_empty() { model.files.clone() } else { models }
}

// --- MakerWorld --------------------------------------------------------------------------

const MW_API: &str = "https://api.bambulab.com/v1/design-service";
const MW_PAGE: usize = 50;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MwCollection {
    id: i64,
    title: String,
    slug: String,
    description: String,
    creator: Option<MwUser>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MwUser {
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MwPage {
    hits: Vec<MwDesign>,
    total: usize,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MwDesign {
    id: i64,
    title: String,
    slug: String,
    cover: String,
    design_creator: Option<MwUser>,
}

fn mw_error(message: String) -> ImportError {
    ImportError::Upstream { site: "makerworld", message }
}

async fn mw_get<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, path: &str) -> Result<T> {
    let res = client.get(format!("{MW_API}{path}")).header("accept", "application/json").send().await?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ImportError::NotFound);
    }
    if !status.is_success() {
        return Err(mw_error(format!("{path} returned {status}")));
    }
    res.json().await.map_err(|e| mw_error(format!("{path}: {e}")))
}

fn mw_model(d: MwDesign) -> CollectionModel {
    let slug = if d.slug.is_empty() { String::new() } else { format!("-{}", d.slug) };
    CollectionModel {
        url: format!("https://makerworld.com/en/models/{}{slug}", d.id),
        id: d.id.to_string(),
        name: d.title.trim().to_string(),
        cover: Some(d.cover).filter(|c| !c.is_empty()),
        author: d.design_creator.map(|c| c.name).filter(|n| !n.is_empty()),
    }
}

async fn makerworld(client: &reqwest::Client, id: &str) -> Result<RemoteCollection> {
    let c: MwCollection = mw_get(client, &format!("/favorites/{id}")).await?;
    if c.id == 0 {
        return Err(ImportError::NotFound);
    }
    let mut models = Vec::new();
    let mut truncated = false;
    loop {
        let page: MwPage = mw_get(client, &format!("/favorites/{id}/designs?limit={MW_PAGE}&offset={}", models.len())).await?;
        let got = page.hits.len();
        models.extend(page.hits.into_iter().map(mw_model));
        if models.len() >= MAX_MODELS {
            truncated = page.total > MAX_MODELS;
            models.truncate(MAX_MODELS);
            break;
        }
        if got == 0 || models.len() >= page.total {
            break;
        }
    }
    let slug = if c.slug.is_empty() { String::new() } else { format!("-{}", c.slug) };
    Ok(RemoteCollection {
        source: Source::Makerworld,
        source_id: c.id.to_string(),
        url: format!("https://makerworld.com/en/collections/{}{slug}", c.id),
        name: c.title.trim().to_string(),
        description: html_to_markdown(&c.description),
        author: c.creator.map(|u| u.name).filter(|n| !n.is_empty()),
        models,
        truncated,
    })
}

// --- Thingiverse -------------------------------------------------------------------------

const TV_API: &str = "https://api.thingiverse.com";
const TV_AGENT: &str = concat!("LiMa/", env!("CARGO_PKG_VERSION"));
const TV_PAGE: usize = 30;

#[derive(Deserialize)]
struct TvCollection {
    id: i64,
    name: String,
    description_html: Option<String>,
    description: Option<String>,
    creator: Option<TvUser>,
}

#[derive(Deserialize)]
struct TvUser {
    name: Option<String>,
    public_url: Option<String>,
}

#[derive(Deserialize)]
struct TvThing {
    id: i64,
    name: String,
    public_url: Option<String>,
    thumbnail: Option<String>,
    creator: Option<TvUser>,
}

async fn tv_get<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, token: &str, path: &str) -> Result<T> {
    let res = client.get(format!("{TV_API}{path}")).header(USER_AGENT, TV_AGENT).header(AUTHORIZATION, format!("Bearer {token}")).send().await?;
    let status = res.status();
    match status.as_u16() {
        200..=299 => res.json().await.map_err(|e| ImportError::Upstream { site: "thingiverse", message: format!("{path}: {e}") }),
        404 => Err(ImportError::NotFound),
        401 | 403 => Err(ImportError::TokenRejected),
        _ => Err(ImportError::Upstream { site: "thingiverse", message: format!("{path} returned {status}") }),
    }
}

async fn thingiverse(client: &reqwest::Client, id: &str, token: &str) -> Result<RemoteCollection> {
    let c: TvCollection = tv_get(client, token, &format!("/collections/{id}")).await?;
    let mut models = Vec::new();
    let mut truncated = false;
    for page in 1.. {
        let things: Vec<TvThing> = tv_get(client, token, &format!("/collections/{id}/things?page={page}&per_page={TV_PAGE}")).await?;
        let got = things.len();
        models.extend(things.into_iter().map(|t| CollectionModel {
            url: t.public_url.unwrap_or_else(|| format!("https://www.thingiverse.com/thing:{}", t.id)),
            id: t.id.to_string(),
            name: t.name.trim().to_string(),
            cover: t.thumbnail.filter(|u| !u.is_empty()),
            author: t.creator.and_then(|u| u.name),
        }));
        if models.len() >= MAX_MODELS {
            truncated = got == TV_PAGE;
            models.truncate(MAX_MODELS);
            break;
        }
        if got < TV_PAGE {
            break;
        }
    }
    let mut description = html_to_markdown(c.description_html.as_deref().unwrap_or(""));
    if description.is_empty() {
        description = c.description.unwrap_or_default().trim().to_string();
    }
    let owner_page = c.creator.as_ref().and_then(|u| u.public_url.clone()).unwrap_or_else(|| "https://www.thingiverse.com".into());
    Ok(RemoteCollection {
        source: Source::Thingiverse,
        source_id: c.id.to_string(),
        url: format!("{}/collections/{}", owner_page.trim_end_matches('/'), c.id),
        name: c.name.trim().to_string(),
        description,
        author: c.creator.and_then(|u| u.name),
        models,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collection_urls() {
        let mw = Some(CollectionRef { source: Source::Makerworld, id: "118240".into() });
        assert_eq!(parse_collection_url("https://makerworld.com/es/collections/118240-toys"), mw);
        assert_eq!(parse_collection_url("https://makerworld.com/collections/118240"), mw);
        let tv = Some(CollectionRef { source: Source::Thingiverse, id: "3366657".into() });
        assert_eq!(parse_collection_url("https://www.thingiverse.com/creativetools/collections/3366657"), tv);
        assert_eq!(parse_collection_url("https://makerworld.com/en/models/1400373"), None);
        assert_eq!(parse_collection_url("https://makerworld.com/en/@meraner/collections"), None);
        assert_eq!(parse_collection_url("https://www.printables.com/@Prusa/collections/54487"), None);
    }

    #[test]
    fn maps_makerworld_designs() {
        let d: MwDesign = serde_json::from_value(serde_json::json!({
            "id": 1765579, "title": " Shark ", "slug": "shark", "cover": "https://cdn/c.jpeg",
            "designCreator": { "name": "Zylo 3D" }, "likeCount": 3
        }))
        .unwrap();
        let m = mw_model(d);
        assert_eq!(m.url, "https://makerworld.com/en/models/1765579-shark");
        assert_eq!(m.name, "Shark");
        assert_eq!(m.author.as_deref(), Some("Zylo 3D"));
    }

    #[test]
    fn default_files_per_source() {
        let file = |id: &str, kind: &str| RemoteFile { id: id.into(), name: format!("{id}.x"), size: None, kind: kind.into(), group: None };
        let model = |source, files| RemoteModel {
            source,
            source_id: "1".into(),
            url: String::new(),
            name: String::new(),
            description: String::new(),
            author: None,
            license: None,
            tags: vec![],
            images: vec![],
            files,
        };
        let ids = |v: Vec<RemoteFile>| v.into_iter().map(|f| f.id).collect::<Vec<_>>();
        assert_eq!(ids(default_files(&model(Source::Makerworld, vec![file("a", "model"), file("b", "model")]))), vec!["a"]);
        assert_eq!(ids(default_files(&model(Source::Thingiverse, vec![file("a", "other"), file("b", "model")]))), vec!["b"]);
        assert_eq!(ids(default_files(&model(Source::Thingiverse, vec![file("a", "other")]))), vec!["a"]);
    }
}
