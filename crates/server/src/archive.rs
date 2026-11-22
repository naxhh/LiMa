//! Extracting uploaded zips into their bundle before it's imported into a project.

use std::collections::HashSet;
use std::path::Path;

use lima_domain::models::bundle::BundleMeta;
use lima_indexer::archive::{self, ArchiveError, Limits};

#[derive(Debug, Default, serde::Serialize, utoipa::ToSchema)]
pub struct ExtractSummary {
    /// Zip files that were extracted.
    pub archives: usize,
    /// Files taken out of them.
    pub files: usize,
    /// Entries left out (password protected or unsupported compression), as `archive.zip/path`.
    pub skipped: Vec<String>,
}

#[derive(Debug)]
pub enum ExtractError {
    /// The named zip is broken, too big or otherwise unreadable.
    Archive { name: String, error: ArchiveError },
    Io(String),
}

/// Replaces each zip in the bundle with its contents, keeping the archive's folders. Names
/// already used by the bundle or by `existing` (the target project's files) get a ` (2)` suffix.
/// With `keep_zips`, the archives are imported too.
pub async fn extract_bundle_zips(bundle_dir: &Path, existing: &[String], keep_zips: bool) -> Result<ExtractSummary, ExtractError> {
    let meta_path = bundle_dir.join("meta.json");
    let raw = tokio::fs::read_to_string(&meta_path).await.map_err(|e| ExtractError::Io(e.to_string()))?;
    let mut meta: BundleMeta = serde_json::from_str(&raw).map_err(|e| ExtractError::Io(e.to_string()))?;
    let zips: Vec<String> = meta.files.iter().filter(|f| archive::is_zip(&f.name)).map(|f| f.name.clone()).collect();
    let mut summary = ExtractSummary::default();
    if zips.is_empty() {
        return Ok(summary);
    }

    let mut taken: HashSet<String> = meta.files.iter().map(|f| f.name.to_ascii_lowercase()).chain(existing.iter().map(|p| p.to_ascii_lowercase())).collect();
    taken.insert("meta.json".into());
    let mut budget = Limits::default();
    let mut done: Vec<String> = Vec::new();
    let mut written: Vec<String> = Vec::new();

    for name in zips {
        let path = bundle_dir.join(&name);
        let dest = bundle_dir.to_path_buf();
        let limits = budget;
        let mut claimed = std::mem::take(&mut taken);
        let (result, claimed) = tokio::task::spawn_blocking(move || {
            let r = archive::extract(&path, &dest, limits, &mut claimed);
            (r, claimed)
        })
        .await
        .map_err(|e| ExtractError::Io(e.to_string()))?;
        taken = claimed;
        let extracted = match result {
            Ok(x) => x,
            Err(error) => {
                // Undo earlier archives too, so the bundle is left as it was uploaded.
                for rel in &written {
                    let _ = tokio::fs::remove_file(bundle_dir.join(rel)).await;
                }
                return Err(ExtractError::Archive { name, error });
            }
        };
        written.extend(extracted.files.iter().cloned());

        for rel in &extracted.files {
            let m = crate::bundles::file_meta(bundle_dir.join(rel), rel).await.map_err(|e| ExtractError::Io(e.to_string()))?;
            budget.max_bytes = budget.max_bytes.saturating_sub(m.size.max(0) as u64);
            meta.files.push(m);
        }
        budget.max_files = budget.max_files.saturating_sub(extracted.files.len());
        summary.archives += 1;
        summary.files += extracted.files.len();
        summary.skipped.extend(extracted.skipped.into_iter().map(|s| format!("{name}/{s}")));
        done.push(name);
    }

    // Only once every archive extracted, so a failure leaves the bundle as it was uploaded.
    if !keep_zips {
        meta.files.retain(|f| !done.contains(&f.name));
        for name in &done {
            let _ = tokio::fs::remove_file(bundle_dir.join(name)).await;
        }
    }

    let json = serde_json::to_vec_pretty(&meta).map_err(|e| ExtractError::Io(e.to_string()))?;
    tokio::fs::write(&meta_path, json).await.map_err(|e| ExtractError::Io(e.to_string()))?;
    Ok(summary)
}
