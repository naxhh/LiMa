//! Printables, through its public GraphQL API. Metadata and downloads both work
//! without an account.

use serde::Deserialize;
use serde_json::json;

use crate::{ImportError, NameSet, RemoteFile, RemoteImage, RemoteModel, Result, Source, file_kind, html_to_text};

const API: &str = "https://api.printables.com/graphql/";
const MEDIA: &str = "https://media.printables.com/";

const QUERY: &str = r#"
query Print($id: ID!) {
  print(id: $id) {
    id name slug summary description
    license { name }
    user { publicUsername }
    tags { name }
    images { filePath }
    stls { id name fileSize }
    gcodes { id name fileSize }
    slas { id name fileSize }
    otherFiles { id name fileSize }
  }
}"#;

const DOWNLOAD: &str = r#"
mutation Download($id: ID!, $printId: ID!, $fileType: DownloadFileTypeEnum!) {
  getDownloadLink(id: $id, printId: $printId, fileType: $fileType, source: model_detail) {
    ok
    errors { field messages }
    output { link }
  }
}"#;

#[derive(Deserialize)]
struct Gql<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct PrintData {
    print: Option<Print>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Print {
    id: String,
    name: String,
    slug: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    license: Option<Named>,
    user: Option<User>,
    #[serde(default)]
    tags: Vec<Named>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    stls: Vec<File>,
    #[serde(default)]
    gcodes: Vec<File>,
    #[serde(default)]
    slas: Vec<File>,
    #[serde(default)]
    other_files: Vec<File>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    public_username: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    file_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    id: String,
    name: String,
    file_size: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadData {
    get_download_link: DownloadResult,
}

#[derive(Deserialize)]
struct DownloadResult {
    ok: bool,
    errors: Option<serde_json::Value>,
    output: Option<DownloadOutput>,
}

#[derive(Deserialize)]
struct DownloadOutput {
    link: String,
}

async fn gql<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, query: &str, variables: serde_json::Value) -> Result<T> {
    let res = client
        .post(API)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await?;
    let status = res.status();
    let body: Gql<T> = res.json().await.map_err(|e| upstream(format!("{status}: {e}")))?;
    if let Some(err) = body.errors.first() {
        return Err(upstream(err.message.clone()));
    }
    body.data.ok_or_else(|| upstream(format!("{status}: empty response")))
}

fn upstream(message: String) -> ImportError {
    ImportError::Upstream { site: "printables", message }
}

pub async fn fetch(client: &reqwest::Client, id: &str) -> Result<RemoteModel> {
    let data: PrintData = gql(client, QUERY, json!({ "id": id })).await?;
    let print = data.print.ok_or(ImportError::NotFound)?;
    Ok(to_model(print))
}

pub(crate) fn to_model(p: Print) -> RemoteModel {
    let mut names = NameSet::default();

    let images = p
        .images
        .iter()
        .map(|img| {
            let base = img.file_path.rsplit('/').next().unwrap_or("image.jpg");
            RemoteImage { url: format!("{MEDIA}{}", img.file_path.trim_start_matches('/')), name: names.claim(base) }
        })
        .collect();

    // File ids are only unique per type, so the type travels in the id.
    let groups: [(&str, &str, &Vec<File>); 4] = [
        ("stl", "Models", &p.stls),
        ("gcode", "G-code", &p.gcodes),
        ("sla", "SLA", &p.slas),
        ("other", "Other files", &p.other_files),
    ];
    let files = groups
        .iter()
        .flat_map(|(ty, label, list)| {
            list.iter().map(|f| (ty.to_string(), label.to_string(), f)).collect::<Vec<_>>()
        })
        .map(|(ty, label, f)| {
            let kind = if ty == "gcode" || ty == "sla" { "gcode" } else { file_kind(&f.name) };
            RemoteFile {
                id: format!("{ty}:{}", f.id),
                name: names.claim(&f.name),
                size: f.file_size,
                kind: kind.to_string(),
                group: Some(label),
            }
        })
        .collect();

    let mut description = html_to_text(p.description.as_deref().unwrap_or(""));
    if description.is_empty() {
        description = p.summary.clone().unwrap_or_default().trim().to_string();
    }

    let slug = p.slug.as_deref().map(|s| format!("-{s}")).unwrap_or_default();
    RemoteModel {
        source: Source::Printables,
        url: format!("https://www.printables.com/model/{}{slug}", p.id),
        source_id: p.id,
        name: p.name.trim().to_string(),
        description,
        author: p.user.and_then(|u| u.public_username),
        license: p.license.map(|l| l.name),
        tags: p.tags.into_iter().map(|t| t.name).collect(),
        images,
        files,
    }
}

/// Signed download link for one file (valid for about a day).
pub async fn download_url(client: &reqwest::Client, print_id: &str, file: &RemoteFile) -> Result<String> {
    let (ty, id) = file.id.split_once(':').ok_or_else(|| ImportError::UnknownFile(file.id.clone()))?;
    let data: DownloadData = gql(
        client,
        DOWNLOAD,
        json!({ "id": id, "printId": print_id, "fileType": ty }),
    )
    .await?;
    let result = data.get_download_link;
    match (result.ok, result.output) {
        (true, Some(out)) => Ok(out.link),
        _ => Err(upstream(format!(
            "no download link for {}: {}",
            file.name,
            result.errors.map(|e| e.to_string()).unwrap_or_default()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_print() {
        let print: Print = serde_json::from_value(json!({
            "id": "3161", "name": " 3D BENCHY ", "slug": "3d-benchy",
            "summary": "Benchmark boat", "description": "<p>Hello <b>world</b></p>",
            "license": { "name": "Public Domain" },
            "user": { "publicUsername": "Prusa Research" },
            "tags": [{ "name": "boat" }],
            "images": [
                { "filePath": "media/prints/3161/images/1_a/benchy.jpg" },
                { "filePath": "media/prints/3161/images/2_b/benchy.jpg" }
            ],
            "stls": [{ "id": "49068", "name": "3dbenchy.stl", "fileSize": 11285384 }],
            "gcodes": [{ "id": "13745", "name": "benchy.gcode", "fileSize": null }],
            "slas": [], "otherFiles": [{ "id": "7", "name": "readme.pdf", "fileSize": 10 }]
        }))
        .unwrap();

        let m = to_model(print);
        assert_eq!(m.name, "3D BENCHY");
        assert_eq!(m.url, "https://www.printables.com/model/3161-3d-benchy");
        assert_eq!(m.author.as_deref(), Some("Prusa Research"));
        assert_eq!(m.description, "Hello world");
        assert_eq!(m.images[0].url, "https://media.printables.com/media/prints/3161/images/1_a/benchy.jpg");
        assert_eq!(m.images[1].name, "benchy-2.jpg");
        let files: Vec<(&str, &str, &str)> = m.files.iter().map(|f| (f.id.as_str(), f.name.as_str(), f.kind.as_str())).collect();
        assert_eq!(
            files,
            vec![
                ("stl:49068", "3dbenchy.stl", "model"),
                ("gcode:13745", "benchy.gcode", "gcode"),
                ("other:7", "readme.pdf", "other"),
            ]
        );
    }
}
