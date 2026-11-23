//! Fetch model pages from external sites (Printables, MakerWorld, Thingiverse) so they can be
//! imported into the library from a URL.
//!
//! Every site goes through its JSON API rather than its HTML pages: the pages sit
//! behind bot protection, the APIs answer plain HTTP clients.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

pub mod bambu;
pub mod collections;
pub mod makerworld;
pub mod printables;
pub mod thingiverse;
mod text;

pub use text::html_to_markdown;
/// HTTP client type used by every importer.
pub use reqwest::Client as HttpClient;

/// Browser-like user agent; both APIs refuse obviously scripted clients less often with it.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Printables,
    Makerworld,
    Thingiverse,
}

impl Source {
    /// Whether downloading files from this source needs a logged-in account.
    pub fn downloads_need_login(self) -> bool {
        matches!(self, Source::Makerworld)
    }
}

/// A model page recognised from a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub source: Source,
    pub id: String,
}

/// Everything we can learn about a model before downloading anything.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteModel {
    pub source: Source,
    pub source_id: String,
    /// Canonical page URL.
    pub url: String,
    pub name: String,
    /// Plain-text description (HTML from the site converted to text).
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub tags: Vec<String>,
    pub images: Vec<RemoteImage>,
    pub files: Vec<RemoteFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteImage {
    pub url: String,
    /// File name to store it under (unique within the model).
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteFile {
    /// Opaque id understood by the source's `download_file`.
    pub id: String,
    /// File name to store it under (unique within the model).
    pub name: String,
    pub size: Option<i64>,
    /// `model`, `gcode` or `other`; used to preselect files in the UI.
    pub kind: String,
    /// Optional grouping label, e.g. the MakerWorld print profile.
    pub group: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("unsupported URL: only Printables, MakerWorld and Thingiverse model links are supported")]
    UnsupportedUrl,
    #[error("Thingiverse imports need an app token (LIMA_THINGIVERSE_TOKEN)")]
    TokenMissing,
    #[error("the site rejected the configured token")]
    TokenRejected,
    #[error("model not found")]
    NotFound,
    #[error("login required to download from this site")]
    LoginRequired,
    #[error("unknown file id {0}")]
    UnknownFile(String),
    #[error("unexpected response from {site}: {message}")]
    Upstream { site: &'static str, message: String },
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ImportError>;

/// HTTP client shared by all importers.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10))
        // Per read, not per request: large downloads may take minutes but should never stall.
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("static reqwest config is valid")
}

/// Credentials some sites need.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tokens<'a> {
    /// Bambu access token, for MakerWorld downloads (from the user's browser).
    pub bambu: Option<&'a str>,
    /// Thingiverse app token, for everything on Thingiverse (server configuration).
    pub thingiverse: Option<&'a str>,
}

/// Recognises a Printables, MakerWorld or Thingiverse model URL.
pub fn parse_url(raw: &str) -> Option<ModelRef> {
    let url = url::Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.trim_start_matches("www.").to_ascii_lowercase();
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();

    // Both sites put the model under ".../model(s)/<id>-<slug>", optionally after a
    // language prefix ("/es/model/...", "/en/models/...").
    // Thingiverse: "/thing:<id>", optionally followed by "/files" and the like.
    if host == "thingiverse.com" {
        let id: String = segments.first()?.strip_prefix("thing:")?.chars().take_while(|c| c.is_ascii_digit()).collect();
        return (!id.is_empty()).then_some(ModelRef { source: Source::Thingiverse, id });
    }

    let (source, marker) = match host.as_str() {
        "printables.com" => (Source::Printables, "model"),
        "makerworld.com" => (Source::Makerworld, "models"),
        _ => return None,
    };
    let pos = segments.iter().position(|s| *s == marker)?;
    let slug = segments.get(pos + 1)?;
    let id: String = slug.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(ModelRef { source, id })
}

/// Fetches the model metadata and file list. Never needs a user login; Thingiverse
/// needs its app token.
pub async fn fetch_model(client: &reqwest::Client, model: &ModelRef, tokens: Tokens<'_>) -> Result<RemoteModel> {
    match model.source {
        Source::Printables => printables::fetch(client, &model.id).await,
        Source::Makerworld => makerworld::fetch(client, &model.id).await,
        Source::Thingiverse => thingiverse::fetch(client, &model.id, tokens.thingiverse).await,
    }
}

/// Downloads one file of `model` into `dir` under `file.name`.
pub async fn download_file(
    client: &reqwest::Client,
    model: &RemoteModel,
    file: &RemoteFile,
    tokens: Tokens<'_>,
    dir: &Path,
) -> Result<PathBuf> {
    let dest = dir.join(&file.name);
    let url = match model.source {
        Source::Printables => printables::download_url(client, &model.source_id, file).await?,
        Source::Makerworld => makerworld::download_url(client, file, tokens.bambu.ok_or(ImportError::LoginRequired)?).await?,
        Source::Thingiverse => return thingiverse::download(client, file, tokens.thingiverse.ok_or(ImportError::TokenMissing)?, &dest).await,
    };
    download_to(client, &url, &dest).await
}

/// Downloads one of `model`'s images into `dir` under `image.name`.
pub async fn download_image(client: &reqwest::Client, model: &RemoteModel, image: &RemoteImage, dir: &Path) -> Result<PathBuf> {
    let dest = dir.join(&image.name);
    match model.source {
        Source::Thingiverse => thingiverse::download_image(client, &image.url, &dest).await,
        _ => download_to(client, &image.url, &dest).await,
    }
}

/// Downloads a public URL (images) into `dest`.
pub async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> Result<PathBuf> {
    download_with(client.get(url), dest).await
}

/// Sends `request` and streams the response body into `dest`.
pub(crate) async fn download_with(request: reqwest::RequestBuilder, dest: &Path) -> Result<PathBuf> {
    let res = request.send().await?;
    if !res.status().is_success() {
        return Err(ImportError::Upstream {
            site: "download",
            message: format!("{} returned {}", res.url().host_str().unwrap_or_default(), res.status()),
        });
    }
    // Keep the file's original date when the server tells it.
    let last_modified = res
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| httpdate::parse_http_date(v).ok());
    let mut out = tokio::fs::File::create(dest).await?;
    let mut stream = res.bytes_stream();
    let written: std::result::Result<(), ImportError> = async {
        while let Some(chunk) = stream.next().await {
            out.write_all(&chunk?).await?;
        }
        out.flush().await?;
        Ok(())
    }
    .await;
    if let Err(e) = written {
        drop(out);
        let _ = tokio::fs::remove_file(dest).await;
        return Err(e);
    }
    if let Some(time) = last_modified
        && let Err(e) = out.into_std().await.set_modified(time)
    {
        tracing::debug!("Could not set the date of {}: {e}", dest.display());
    }
    Ok(dest.to_path_buf())
}

/// Makes a site-provided name safe to use as a file name in a project folder.
pub fn sanitize_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if cleaned.is_empty() { "file".to_string() } else { cleaned }
}

/// Hands out unique file names within one model ("a.png", "a-2.png", ...),
/// comparing case-insensitively.
#[derive(Default)]
pub(crate) struct NameSet(std::collections::HashSet<String>);

impl NameSet {
    pub(crate) fn claim(&mut self, raw: &str) -> String {
        let name = sanitize_file_name(raw);
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut candidate = name;
        let mut n = 2;
        while !self.0.insert(candidate.to_ascii_lowercase()) {
            candidate = format!("{stem}-{n}{ext}");
            n += 1;
        }
        candidate
    }
}

/// Classifies a file name for preselection: printable geometry, sliced gcode, or other.
pub(crate) fn file_kind(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "stl" | "3mf" | "obj" | "step" | "stp" | "f3d" | "scad" | "amf" | "ply" | "zip" => "model",
        "gcode" | "bgcode" | "sl1" | "sl1s" | "ctb" => "gcode",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_printables_urls() {
        let want = Some(ModelRef { source: Source::Printables, id: "3161".into() });
        assert_eq!(parse_url("https://www.printables.com/model/3161-3d-benchy"), want);
        assert_eq!(parse_url("https://printables.com/es/model/3161-3d-benchy/files"), want);
        assert_eq!(parse_url("  https://www.printables.com/model/3161  "), want);
    }

    #[test]
    fn parses_makerworld_urls() {
        let want = Some(ModelRef { source: Source::Makerworld, id: "1400373".into() });
        assert_eq!(parse_url("https://makerworld.com/en/models/1400373-self-watering-seed-starter"), want);
        assert_eq!(parse_url("https://makerworld.com/models/1400373?from=search#profileId-1452154"), want);
    }

    #[test]
    fn parses_thingiverse_urls() {
        let want = Some(ModelRef { source: Source::Thingiverse, id: "763622".into() });
        assert_eq!(parse_url("https://www.thingiverse.com/thing:763622"), want);
        assert_eq!(parse_url("https://thingiverse.com/thing:763622/files"), want);
        assert_eq!(parse_url("https://www.thingiverse.com/thing:763622?utm=x#comments"), want);
        assert_eq!(parse_url("https://www.thingiverse.com/creativetools/designs"), None);
        assert_eq!(parse_url("https://www.thingiverse.com/thing:abc"), None);
    }

    #[test]
    fn rejects_other_urls() {
        assert_eq!(parse_url("https://www.printables.com/@someone"), None);
        assert_eq!(parse_url("https://makerworld.com/en/models/abc"), None);
        assert_eq!(parse_url("ftp://printables.com/model/1"), None);
        assert_eq!(parse_url("not a url"), None);
    }

    #[test]
    fn unique_names() {
        let mut names = NameSet::default();
        assert_eq!(names.claim("a.png"), "a.png");
        assert_eq!(names.claim("A.png"), "A-2.png");
        assert_eq!(names.claim("a.png"), "a-3.png");
        assert_eq!(names.claim("../etc/passwd"), "_etc_passwd");
        assert_eq!(names.claim("   "), "file");
    }
}

#[cfg(test)]
mod download_tests {
    use std::io::{Read, Write};
    use std::time::{Duration, UNIX_EPOCH};

    /// Serves one canned HTTP response on a random local port.
    fn serve_once(response: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = conn.read(&mut buf);
            conn.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{addr}/file.stl")
    }

    #[tokio::test]
    async fn keeps_the_servers_last_modified_date() {
        let url = serve_once(
            "HTTP/1.1 200 OK\r\nLast-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\nContent-Length: 5\r\nConnection: close\r\n\r\nsolid",
        );
        let dir = std::env::temp_dir().join(format!("lima-dl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("file.stl");

        super::download_to(&reqwest::Client::new(), &url, &dest).await.unwrap();

        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "solid");
        let modified = std::fs::metadata(&dest).unwrap().modified().unwrap();
        assert_eq!(modified, UNIX_EPOCH + Duration::from_secs(1_445_412_480));
        let _ = std::fs::remove_dir_all(dir);
    }
}
