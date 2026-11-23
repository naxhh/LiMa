//! Export the user's metadata to a [`MetadataBackup`] and restore it onto the
//! projects that the indexer found on disk.

use std::collections::{BTreeMap, HashMap, HashSet};

use lima_domain::models::metadata::{
    BackupCollectionSource, BackupDismissal, BackupSourceItem,
    BackupCollection, BackupMembership, BackupProject, BackupSource, BackupTag, METADATA_FORMAT, METADATA_VERSION,
    MetadataBackup, RenamedMatch, RestoreReport,
};
use sqlx::{FromRow, Pool, Sqlite, Transaction};
use uuid::Uuid;

use crate::queries::collections::FAVOURITES_KEY;

/// A renamed folder is only matched when this share of files (by hash) is the same.
const RENAME_MATCH_MIN_SIMILARITY: f64 = 0.8;

#[derive(FromRow)]
struct ProjectExportRow {
    id: String,
    folder_path: String,
    name: String,
    description: String,
    main_image_path: Option<String>,
    main_image_hash: Option<String>,
    source_url: Option<String>,
    source_author: Option<String>,
    source_license: Option<String>,
    created_at: String,
}

pub async fn export_metadata(pool: &Pool<Sqlite>, now: &str) -> Result<MetadataBackup, sqlx::Error> {
    let tags = sqlx::query_as::<_, (String, String)>("SELECT name, color FROM tags ORDER BY name COLLATE NOCASE")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(name, color)| BackupTag { name, color })
        .collect();

    let collections =
        sqlx::query_as::<_, (String, String, Option<String>, String)>(
            "SELECT name, description, system_key, created_at FROM collections ORDER BY system_key IS NULL, name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(name, description, system_key, created_at)| BackupCollection { name, description, system_key, created_at })
        .collect();

    let rows = sqlx::query_as::<_, ProjectExportRow>(
        r#"
        SELECT p.id, p.folder_path, p.name, p.description,
               a.file_path AS main_image_path, a.file_hash AS main_image_hash,
               p.source_url, p.source_author, p.source_license, p.created_at
        FROM projects p
        LEFT JOIN assets a ON a.id = p.main_image_id
        ORDER BY p.folder_path
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut tags_by_project: HashMap<String, Vec<String>> = HashMap::new();
    for (pid, name) in sqlx::query_as::<_, (String, String)>(
        "SELECT pt.project_id, t.name FROM project_tags pt JOIN tags t ON t.id = pt.tag_id ORDER BY t.name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?
    {
        tags_by_project.entry(pid).or_default().push(name);
    }

    let mut collections_by_project: HashMap<String, Vec<BackupMembership>> = HashMap::new();
    for (pid, name, system_key, added_at) in sqlx::query_as::<_, (String, String, Option<String>, String)>(
        r#"
        SELECT cp.project_id, c.name, c.system_key, cp.created_at
        FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id
        ORDER BY c.system_key IS NULL, c.name COLLATE NOCASE
        "#,
    )
    .fetch_all(pool)
    .await?
    {
        let m = match system_key {
            Some(key) => BackupMembership { name: None, system_key: Some(key), added_at },
            None => BackupMembership { name: Some(name), system_key: None, added_at },
        };
        collections_by_project.entry(pid).or_default().push(m);
    }

    let mut hashes_by_project: HashMap<String, Vec<String>> = HashMap::new();
    for (pid, hash) in sqlx::query_as::<_, (String, String)>(
        "SELECT project_id, file_hash FROM assets WHERE file_hash IS NOT NULL ORDER BY file_hash",
    )
    .fetch_all(pool)
    .await?
    {
        hashes_by_project.entry(pid).or_default().push(hash);
    }

    let projects = rows
        .into_iter()
        .map(|r| {
            let mut file_hashes = hashes_by_project.remove(&r.id).unwrap_or_default();
            file_hashes.dedup();
            BackupProject {
                tags: tags_by_project.remove(&r.id).unwrap_or_default(),
                collections: collections_by_project.remove(&r.id).unwrap_or_default(),
                source: r.source_url.map(|url| BackupSource { url, author: r.source_author, license: r.source_license }),
                folder_path: r.folder_path,
                name: r.name,
                description: r.description,
                main_image: r.main_image_path,
                main_image_hash: r.main_image_hash,
                file_hashes,
                created_at: r.created_at,
            }
        })
        .collect();

    let ignored_folders = crate::queries::ignored_folders::list_ignored(pool)
        .await?
        .into_iter()
        .map(|f| f.folder_path)
        .collect();

    let folders: HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, folder_path FROM projects").fetch_all(pool).await?.into_iter().collect();

    let mut collection_sources = Vec::new();
    for (id, collection, source, source_id, url, keep_in_sync) in sqlx::query_as::<_, (String, String, String, String, String, bool)>(
        "SELECT s.id, c.name, s.source, s.source_id, s.url, s.keep_in_sync FROM collection_sources s JOIN collections c ON c.id = s.collection_id ORDER BY c.name",
    )
    .fetch_all(pool)
    .await?
    {
        let items = crate::queries::collection_sources::items(pool, &id)
            .await?
            .into_iter()
            .map(|i| BackupSourceItem {
                project_folder: i.project_id.as_ref().and_then(|p| folders.get(p).cloned()),
                model_id: i.model_id,
                model_url: i.model_url,
                name: i.name,
                imported: i.imported,
                excluded: i.excluded,
            })
            .collect();
        collection_sources.push(BackupCollectionSource { collection, source, source_id, url, keep_in_sync, items });
    }

    // Dismissed groups name projects by id, which a rebuilt database doesn't keep.
    let duplicate_dismissals = sqlx::query_scalar::<_, String>("SELECT key FROM duplicate_dismissals ORDER BY key")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|key| {
            if let Some(hash) = key.strip_prefix("file:") {
                return Some(BackupDismissal { project_folders: vec![], file_hash: Some(hash.to_string()) });
            }
            let ids = key.strip_prefix("projects:")?;
            let project_folders: Option<Vec<String>> = ids.split(',').map(|id| folders.get(id).cloned()).collect();
            Some(BackupDismissal { project_folders: project_folders?, file_hash: None })
        })
        .collect();

    Ok(MetadataBackup {
        format: METADATA_FORMAT.to_string(),
        version: METADATA_VERSION,
        exported_at: now.to_string(),
        tags,
        collections,
        projects,
        ignored_folders,
        collection_sources,
        duplicate_dismissals,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum RestoreError {
    /// Not a LiMa metadata file, or a newer version than this build understands.
    UnsupportedFormat(String),
    Db(String),
}

impl From<sqlx::Error> for RestoreError {
    fn from(e: sqlx::Error) -> Self {
        RestoreError::Db(e.to_string())
    }
}

struct LibraryProject {
    id: String,
    folder_path: String,
    /// file_path -> asset id, for images.
    images: HashMap<String, String>,
    /// file hash -> asset id, for images.
    image_hashes: HashMap<String, String>,
    hashes: HashSet<String>,
}

/// Re-attaches backed-up metadata to the projects currently in the database.
///
/// Projects are matched by folder; a backed-up folder that no longer exists is
/// matched to an unclaimed library folder holding (almost) the same files.
/// Names, descriptions, covers and sources from the backup overwrite the current
/// ones; tags and collection memberships are added to what's already there.
/// A backup holding only one project, with just the tags and collections it uses,
/// so restoring it later recreates exactly that project's metadata.
pub async fn export_project(pool: &Pool<Sqlite>, project_id: &str, now: &str) -> Result<Option<MetadataBackup>, sqlx::Error> {
    let Some(folder) = sqlx::query_scalar::<_, String>("SELECT folder_path FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let mut backup = export_metadata(pool, now).await?;
    backup.projects.retain(|p| p.folder_path == folder);
    let Some(project) = backup.projects.first() else { return Ok(None) };
    let tags: HashSet<&str> = project.tags.iter().map(String::as_str).collect();
    let collections: HashSet<&str> = project.collections.iter().filter_map(|m| m.name.as_deref()).collect();
    let tags_keep: Vec<BackupTag> = backup.tags.iter().filter(|t| tags.contains(t.name.as_str())).cloned().collect();
    let collections_keep: Vec<BackupCollection> = backup
        .collections
        .iter()
        .filter(|c| c.system_key.is_some() || collections.contains(c.name.as_str()))
        .cloned()
        .collect();
    backup.tags = tags_keep;
    backup.collections = collections_keep;
    backup.ignored_folders.clear();
    backup.collection_sources.clear();
    backup.duplicate_dismissals.clear();
    Ok(Some(backup))
}

/// With `dry_run` nothing is written and the report says what would change.
pub async fn restore_metadata(
    pool: &Pool<Sqlite>,
    backup: &MetadataBackup,
    dry_run: bool,
    now: &str,
) -> Result<RestoreReport, RestoreError> {
    if backup.format != METADATA_FORMAT {
        return Err(RestoreError::UnsupportedFormat(format!("not a LiMa metadata backup (format {:?})", backup.format)));
    }
    if backup.version > METADATA_VERSION {
        return Err(RestoreError::UnsupportedFormat(format!(
            "backup version {} is newer than this LiMa understands ({})",
            backup.version, METADATA_VERSION
        )));
    }

    let library = load_library(pool).await?;
    let by_folder: HashMap<&str, usize> = library.iter().enumerate().map(|(i, p)| (p.folder_path.as_str(), i)).collect();
    let backup_folders: HashSet<&str> = backup.projects.iter().map(|p| p.folder_path.as_str()).collect();

    let mut report = RestoreReport { dry_run, projects_in_backup: backup.projects.len(), ..Default::default() };

    // Pair each backup project with a library project.
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut pairs: Vec<(&BackupProject, usize)> = Vec::new();
    let mut leftovers: Vec<&BackupProject> = Vec::new();
    for bp in &backup.projects {
        match by_folder.get(bp.folder_path.as_str()) {
            Some(&i) => {
                claimed.insert(i);
                pairs.push((bp, i));
                report.matched_by_folder += 1;
            }
            None => leftovers.push(bp),
        }
    }
    // Renamed folders: only library folders the backup doesn't know about are candidates.
    let candidates: Vec<usize> = (0..library.len())
        .filter(|i| !claimed.contains(i) && !backup_folders.contains(library[*i].folder_path.as_str()))
        .collect();
    for bp in leftovers {
        match best_rename_match(bp, &library, &candidates, &claimed) {
            Some(i) => {
                claimed.insert(i);
                report.matched_by_files.push(RenamedMatch {
                    backup_folder: bp.folder_path.clone(),
                    library_folder: library[i].folder_path.clone(),
                });
                pairs.push((bp, i));
            }
            None => report.unmatched.push(bp.folder_path.clone()),
        }
    }

    let mut tx = pool.begin().await?;

    // Tags keep their backed-up colors when they have to be created.
    let mut tag_ids: HashMap<String, String> = HashMap::new(); // lowercase name -> id
    for (id, name) in sqlx::query_as::<_, (String, String)>("SELECT id, name FROM tags").fetch_all(&mut *tx).await? {
        tag_ids.insert(name.to_lowercase(), id);
    }
    let colors: HashMap<String, &str> = backup.tags.iter().map(|t| (t.name.to_lowercase(), t.color.as_str())).collect();

    // Collections, by lowercase name; Favourites by key.
    let mut collection_ids: HashMap<String, String> = HashMap::new();
    let mut favourites_id: Option<String> = None;
    for (id, name, key) in sqlx::query_as::<_, (String, String, Option<String>)>("SELECT id, name, system_key FROM collections")
        .fetch_all(&mut *tx)
        .await?
    {
        if key.as_deref() == Some(FAVOURITES_KEY) {
            favourites_id = Some(id.clone());
        }
        collection_ids.insert(name.to_lowercase(), id);
    }
    let collection_meta: HashMap<String, &BackupCollection> =
        backup.collections.iter().filter(|c| c.system_key.is_none()).map(|c| (c.name.to_lowercase(), c)).collect();

    // Backup folder -> project id, for the parts of the backup that point at projects.
    let mut project_ids: HashMap<String, String> = library.iter().map(|p| (p.folder_path.clone(), p.id.clone())).collect();
    for (bp, i) in &pairs {
        project_ids.insert(bp.folder_path.clone(), library[*i].id.clone());
    }

    for (bp, i) in pairs {
        let lp = &library[i];
        let cover = bp
            .main_image
            .as_ref()
            .and_then(|path| lp.images.get(path))
            .or_else(|| bp.main_image_hash.as_ref().and_then(|h| lp.image_hashes.get(h)))
            .cloned();
        if cover.is_some() {
            report.covers_set += 1;
        }
        let source = bp.source.as_ref();
        sqlx::query(
            r#"
            UPDATE projects SET
                name = ?2,
                description = ?3,
                main_image_id = COALESCE(?4, main_image_id),
                source_url = COALESCE(?5, source_url),
                source_author = COALESCE(?6, source_author),
                source_license = COALESCE(?7, source_license),
                created_at = MIN(created_at, ?8),
                updated_at = ?9
            WHERE id = ?1
            "#,
        )
        .bind(&lp.id)
        .bind(if bp.name.trim().is_empty() { &lp.folder_path } else { &bp.name })
        .bind(&bp.description)
        .bind(cover)
        .bind(source.map(|s| s.url.as_str()))
        .bind(source.and_then(|s| s.author.as_deref()))
        .bind(source.and_then(|s| s.license.as_deref()))
        .bind(&bp.created_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        report.projects_updated += 1;

        for name in &bp.tags {
            let key = name.to_lowercase();
            let tag_id = match tag_ids.get(&key) {
                Some(id) => id.clone(),
                None => {
                    let color = colors.get(&key).copied();
                    let id = crate::queries::tags::create_tag(&mut *tx, name, color, now).await?.id;
                    report.tags_created += 1;
                    tag_ids.insert(key, id.clone());
                    id
                }
            };
            let res = sqlx::query("INSERT OR IGNORE INTO project_tags (project_id, tag_id) VALUES (?1, ?2)")
                .bind(&lp.id)
                .bind(&tag_id)
                .execute(&mut *tx)
                .await?;
            report.tag_links_added += res.rows_affected() as usize;
        }

        for m in &bp.collections {
            let collection_id = if m.system_key.as_deref() == Some(FAVOURITES_KEY) {
                match &favourites_id {
                    Some(id) => id.clone(),
                    None => {
                        let id = create_collection_tx(&mut tx, "Favourites", "", Some(FAVOURITES_KEY), now).await?;
                        report.collections_created += 1;
                        favourites_id = Some(id.clone());
                        id
                    }
                }
            } else if let Some(name) = &m.name {
                let key = name.to_lowercase();
                match collection_ids.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let description = collection_meta.get(&key).map(|c| c.description.as_str()).unwrap_or("");
                        let id = create_collection_tx(&mut tx, name, description, None, now).await?;
                        report.collections_created += 1;
                        collection_ids.insert(key, id.clone());
                        id
                    }
                }
            } else {
                continue;
            };
            let res = sqlx::query(
                "INSERT OR IGNORE INTO collection_projects (collection_id, project_id, created_at) VALUES (?1, ?2, ?3)",
            )
            .bind(&collection_id)
            .bind(&lp.id)
            .bind(&m.added_at)
            .execute(&mut *tx)
            .await?;
            report.collection_links_added += res.rows_affected() as usize;
        }
    }

    // Empty collections from the backup are restored too.
    for (key, c) in &collection_meta {
        if !collection_ids.contains_key(key) {
            let id = create_collection_tx(&mut tx, &c.name, &c.description, None, now).await?;
            collection_ids.insert(key.clone(), id);
            report.collections_created += 1;
        }
    }

    // Folders the user removed from LiMa stay removed (their files are never touched).
    for folder in &backup.ignored_folders {
        let res = sqlx::query("INSERT OR IGNORE INTO ignored_folders (folder_path, created_at) VALUES (?1, ?2)")
            .bind(folder)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        report.ignored_folders_added += res.rows_affected() as usize;
        let res = sqlx::query("DELETE FROM projects WHERE folder_path = ?1").bind(folder).execute(&mut *tx).await?;
        report.projects_hidden += res.rows_affected() as usize;
    }

    // Imported collections keep syncing as before. Ones already linked are left alone.
    for cs in &backup.collection_sources {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collection_sources WHERE source = ?1 AND source_id = ?2")
            .bind(&cs.source)
            .bind(&cs.source_id)
            .fetch_one(&mut *tx)
            .await?;
        if exists > 0 {
            continue;
        }
        let key = cs.collection.to_lowercase();
        let collection_id = match collection_ids.get(&key) {
            Some(id) => id.clone(),
            None => {
                let id = create_collection_tx(&mut tx, &cs.collection, "", None, now).await?;
                report.collections_created += 1;
                collection_ids.insert(key, id.clone());
                id
            }
        };
        let linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collection_sources WHERE collection_id = ?1").bind(&collection_id).fetch_one(&mut *tx).await?;
        if linked > 0 {
            continue;
        }
        let source_ref = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO collection_sources (id, collection_id, source, source_id, url, keep_in_sync, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&source_ref)
        .bind(&collection_id)
        .bind(&cs.source)
        .bind(&cs.source_id)
        .bind(&cs.url)
        .bind(cs.keep_in_sync)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for item in &cs.items {
            let project_id = item.project_folder.as_ref().and_then(|f| project_ids.get(f));
            sqlx::query(
                "INSERT OR IGNORE INTO collection_source_items (source_ref, model_id, model_url, name, project_id, imported, excluded) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(&source_ref)
            .bind(&item.model_id)
            .bind(&item.model_url)
            .bind(&item.name)
            .bind(project_id)
            .bind(item.imported && project_id.is_some())
            .bind(item.excluded)
            .execute(&mut *tx)
            .await?;
        }
        report.collection_sources_added += 1;
    }

    for d in &backup.duplicate_dismissals {
        let key = match (&d.file_hash, d.project_folders.as_slice()) {
            (Some(hash), _) => format!("file:{hash}"),
            (None, folders) if folders.len() > 1 => {
                let ids: Option<Vec<String>> = folders.iter().map(|f| project_ids.get(f).cloned()).collect();
                match ids {
                    Some(ids) => crate::queries::duplicates::project_group_key(&ids),
                    None => continue,
                }
            }
            _ => continue,
        };
        let res = sqlx::query("INSERT OR IGNORE INTO duplicate_dismissals (key, created_at) VALUES (?1, ?2)").bind(&key).bind(now).execute(&mut *tx).await?;
        report.duplicate_dismissals_added += res.rows_affected() as usize;
    }

    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    Ok(report)
}

async fn create_collection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    description: &str,
    system_key: Option<&str>,
    now: &str,
) -> Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO collections (id, name, description, system_key, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(system_key)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn load_library(pool: &Pool<Sqlite>) -> Result<Vec<LibraryProject>, sqlx::Error> {
    let mut projects: BTreeMap<String, LibraryProject> = BTreeMap::new();
    for (id, folder_path) in sqlx::query_as::<_, (String, String)>("SELECT id, folder_path FROM projects").fetch_all(pool).await? {
        projects.insert(
            id.clone(),
            LibraryProject { id, folder_path, images: HashMap::new(), image_hashes: HashMap::new(), hashes: HashSet::new() },
        );
    }
    for (asset_id, project_id, file_path, kind, hash) in sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT id, project_id, file_path, kind, file_hash FROM assets",
    )
    .fetch_all(pool)
    .await?
    {
        let Some(p) = projects.get_mut(&project_id) else { continue };
        if kind == "image" {
            p.images.insert(file_path, asset_id.clone());
            if let Some(h) = &hash {
                p.image_hashes.insert(h.clone(), asset_id);
            }
        }
        if let Some(h) = hash {
            p.hashes.insert(h);
        }
    }
    Ok(projects.into_values().collect())
}

/// The candidate sharing the most files with `bp`, if it's similar enough and unambiguous.
fn best_rename_match(
    bp: &BackupProject,
    library: &[LibraryProject],
    candidates: &[usize],
    claimed: &HashSet<usize>,
) -> Option<usize> {
    if bp.file_hashes.is_empty() {
        return None;
    }
    let wanted: HashSet<&str> = bp.file_hashes.iter().map(String::as_str).collect();
    let mut scored: Vec<(f64, usize)> = candidates
        .iter()
        .filter(|i| !claimed.contains(i))
        .filter_map(|&i| {
            let have = &library[i].hashes;
            if have.is_empty() {
                return None;
            }
            let shared = wanted.iter().filter(|h| have.contains(**h)).count();
            let union = wanted.len() + have.len() - shared;
            let score = shared as f64 / union as f64;
            (score > 0.0).then_some((score, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    match scored.as_slice() {
        [(best, i), rest @ ..] if *best >= RENAME_MATCH_MIN_SIMILARITY && rest.first().is_none_or(|(second, _)| second < best) => {
            Some(*i)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    const T: &str = "2026-01-01T00:00:00Z";

    async fn db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        (db, dir)
    }

    /// A project as the indexer would create it: folder name as name, files with hashes.
    async fn indexed(pool: &Pool<Sqlite>, folder: &str, files: &[(&str, &str, &str)]) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?3)")
            .bind(&id).bind(folder).bind(T).execute(pool).await.unwrap();
        for (path, kind, hash) in files {
            sqlx::query("INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, file_hash, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?5, ?5)")
                .bind(Uuid::new_v4().to_string()).bind(&id).bind(path).bind(kind).bind(T).bind(hash)
                .execute(pool).await.unwrap();
        }
        id
    }

    async fn project_view(pool: &Pool<Sqlite>, folder: &str) -> (String, String, Option<String>, Vec<String>, Vec<String>) {
        let (id, name, description, cover): (String, String, String, Option<String>) = sqlx::query_as(
            "SELECT p.id, p.name, p.description, a.file_path FROM projects p LEFT JOIN assets a ON a.id = p.main_image_id WHERE p.folder_path = ?1",
        )
        .bind(folder)
        .fetch_one(pool)
        .await
        .unwrap();
        let tags: Vec<String> = sqlx::query_scalar(
            "SELECT t.name FROM project_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.project_id = ?1 ORDER BY t.name",
        )
        .bind(&id)
        .fetch_all(pool)
        .await
        .unwrap();
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT c.name FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id WHERE cp.project_id = ?1 ORDER BY c.name",
        )
        .bind(&id)
        .fetch_all(pool)
        .await
        .unwrap();
        (name, description, cover, tags, cols)
    }

    /// Full round trip: organise a library, export, rebuild from "disk" (one folder renamed), restore.
    #[tokio::test]
    async fn export_then_restore_onto_a_rebuilt_database() {
        let (old, dir1) = db().await;
        let pool = old.pool();
        let dragon = indexed(pool, "dragon", &[("dragon.stl", "model", "h1"), ("photo.jpg", "image", "h2")]).await;
        let bins = indexed(pool, "bins", &[("bin.stl", "model", "h3"), ("a.3mf", "model", "h4"), ("b.3mf", "model", "h5"), ("c.3mf", "model", "h6"), ("d.3mf", "model", "h7")]).await;
        indexed(pool, "gone", &[("x.stl", "model", "h9")]).await;

        let photo: String = sqlx::query_scalar("SELECT id FROM assets WHERE file_path = 'photo.jpg'").fetch_one(pool).await.unwrap();
        sqlx::query("UPDATE projects SET name = 'Articulated dragon', description = 'Silk PLA', main_image_id = ?2, source_url = 'https://example.com/m/1' WHERE id = ?1")
            .bind(&dragon).bind(&photo).execute(pool).await.unwrap();
        sqlx::query("UPDATE projects SET name = 'Drawer bins' WHERE id = ?1").bind(&bins).execute(pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::queries::tags::create_tag(&mut *tx, "PLA", Some("#C4F25A"), T).await.unwrap();
        let ids = crate::queries::tags::ensure_tags(&mut tx, &["PLA".into(), "gift".into()], T).await.unwrap();
        crate::queries::tags::set_project_tags(&mut tx, &dragon, &ids).await.unwrap();
        tx.commit().await.unwrap();
        let c = crate::queries::collections::create_collection(pool, "Desk", "Things for the desk", T).await.unwrap();
        crate::queries::collections::add_project(pool, &c.id, &dragon, T).await.unwrap();
        crate::queries::collections::add_project(pool, &c.id, &bins, T).await.unwrap();
        crate::queries::collections::create_collection(pool, "Empty one", "", T).await.unwrap();
        let fav = crate::queries::collections::ensure_favourites(pool, T).await.unwrap();
        crate::queries::collections::add_project(pool, &fav, &dragon, T).await.unwrap();

        let backup = export_metadata(pool, "2026-02-01T00:00:00Z").await.unwrap();
        assert_eq!(backup.projects.len(), 3);
        let json = serde_json::to_string(&backup).unwrap();
        let backup: MetadataBackup = serde_json::from_str(&json).unwrap();

        // New database, rebuilt from disk: "bins" was renamed to "drawer-bins" and one file was added.
        let (new, dir2) = db().await;
        let pool = new.pool();
        indexed(pool, "dragon", &[("dragon.stl", "model", "h1"), ("photo.jpg", "image", "h2")]).await;
        indexed(pool, "drawer-bins", &[("bin.stl", "model", "h3"), ("a.3mf", "model", "h4"), ("b.3mf", "model", "h5"), ("c.3mf", "model", "h6"), ("d.3mf", "model", "h7"), ("e.3mf", "model", "h8")]).await;
        indexed(pool, "unrelated", &[("u.stl", "model", "h99")]).await;

        let dry = restore_metadata(pool, &backup, true, "2026-03-01T00:00:00Z").await.unwrap();
        assert!(dry.dry_run);
        assert_eq!(project_view(pool, "dragon").await.0, "dragon", "dry run writes nothing");

        let r = restore_metadata(pool, &backup, false, "2026-03-01T00:00:00Z").await.unwrap();
        assert_eq!(r.matched_by_folder, 1);
        assert_eq!(r.matched_by_files.len(), 1);
        assert_eq!(r.matched_by_files[0].backup_folder, "bins");
        assert_eq!(r.matched_by_files[0].library_folder, "drawer-bins");
        assert_eq!(r.unmatched, vec!["gone".to_string()]);
        assert_eq!(r.covers_set, 1);
        assert_eq!(r.tags_created, 2);

        let (name, description, cover, tags, cols) = project_view(pool, "dragon").await;
        assert_eq!(name, "Articulated dragon");
        assert_eq!(description, "Silk PLA");
        assert_eq!(cover.as_deref(), Some("photo.jpg"));
        assert_eq!(tags, vec!["PLA", "gift"]);
        assert_eq!(cols, vec!["Desk", "Favourites"]);
        let color: String = sqlx::query_scalar("SELECT color FROM tags WHERE name = 'PLA'").fetch_one(pool).await.unwrap();
        assert_eq!(color, "#C4F25A", "tag colors come back");
        let fav_key: Option<String> = sqlx::query_scalar("SELECT system_key FROM collections WHERE name = 'Favourites'").fetch_one(pool).await.unwrap();
        assert_eq!(fav_key.as_deref(), Some(FAVOURITES_KEY));

        assert_eq!(project_view(pool, "drawer-bins").await.0, "Drawer bins");
        assert_eq!(project_view(pool, "unrelated").await.0, "unrelated");
        let empty: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collections WHERE name = 'Empty one'").fetch_one(pool).await.unwrap();
        assert_eq!(empty, 1);
        let desc: String = sqlx::query_scalar("SELECT description FROM collections WHERE name = 'Desk'").fetch_one(pool).await.unwrap();
        assert_eq!(desc, "Things for the desk");
        let src: Option<String> = sqlx::query_scalar("SELECT source_url FROM projects WHERE folder_path = 'dragon'").fetch_one(pool).await.unwrap();
        assert_eq!(src.as_deref(), Some("https://example.com/m/1"));

        // Search sees the restored names and tags.
        let hits = crate::queries::projects_search::search_projects(pool, "gift", 10, None, &Default::default()).await.unwrap();
        assert_eq!(hits.len(), 1);

        // Restoring twice changes nothing more.
        let again = restore_metadata(pool, &backup, false, "2026-03-02T00:00:00Z").await.unwrap();
        assert_eq!((again.tags_created, again.tag_links_added, again.collections_created, again.collection_links_added), (0, 0, 0, 0));

        let _ = std::fs::remove_dir_all(dir1);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[tokio::test]
    async fn ambiguous_or_weak_renames_are_not_guessed() {
        let (db, dir) = db().await;
        let pool = db.pool();
        indexed(pool, "copy-a", &[("x.stl", "model", "h1"), ("y.stl", "model", "h2")]).await;
        indexed(pool, "copy-b", &[("x.stl", "model", "h1"), ("y.stl", "model", "h2")]).await;
        indexed(pool, "weak", &[("p.stl", "model", "h5"), ("q.stl", "model", "h6"), ("r.stl", "model", "h7")]).await;
        let backup = MetadataBackup {
            format: METADATA_FORMAT.into(),
            version: 1,
            exported_at: T.into(),
            tags: vec![],
            collections: vec![],
            ignored_folders: vec![],
            collection_sources: vec![],
            duplicate_dismissals: vec![],
            projects: vec![
                BackupProject {
                    folder_path: "original".into(), name: "Original".into(), description: String::new(),
                    main_image: None, main_image_hash: None, tags: vec![], collections: vec![], source: None,
                    file_hashes: vec!["h1".into(), "h2".into()], created_at: T.into(),
                },
                BackupProject {
                    folder_path: "partial".into(), name: "Partial".into(), description: String::new(),
                    main_image: None, main_image_hash: None, tags: vec![], collections: vec![], source: None,
                    file_hashes: vec!["h5".into(), "h8".into()], created_at: T.into(),
                },
            ],
        };
        let r = restore_metadata(pool, &backup, false, T).await.unwrap();
        assert!(r.matched_by_files.is_empty(), "two identical candidates, and one weak overlap");
        assert_eq!(r.unmatched.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn rejects_foreign_or_future_files() {
        let (db, dir) = db().await;
        let mut b = export_metadata(db.pool(), T).await.unwrap();
        b.version = METADATA_VERSION + 1;
        assert!(matches!(restore_metadata(db.pool(), &b, true, T).await, Err(RestoreError::UnsupportedFormat(_))));
        b.version = 1;
        b.format = "something-else".into();
        assert!(matches!(restore_metadata(db.pool(), &b, true, T).await, Err(RestoreError::UnsupportedFormat(_))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn ignored_folders_round_trip_and_stay_hidden() {
        let (db, dir) = db().await;
        let pool = db.pool();
        indexed(pool, "kept", &[("a.stl", "model", "h1")]).await;
        indexed(pool, "removed", &[("b.stl", "model", "h2")]).await;
        let id: String = sqlx::query_scalar("SELECT id FROM projects WHERE folder_path = 'removed'").fetch_one(pool).await.unwrap();
        crate::queries::projects_delete::remove_project_keep_files(pool, &id, T).await.unwrap();

        let backup = export_metadata(pool, T).await.unwrap();
        assert_eq!(backup.ignored_folders, vec!["removed".to_string()]);

        // A rebuilt database: the full sync brought "removed" back before the restore.
        crate::queries::ignored_folders::unignore(pool, "removed").await.unwrap();
        indexed(pool, "removed", &[("b.stl", "model", "h2")]).await;

        let dry = restore_metadata(pool, &backup, true, T).await.unwrap();
        assert_eq!((dry.ignored_folders_added, dry.projects_hidden), (1, 1));
        let count = |sql: &'static str| async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await.unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM projects WHERE folder_path = 'removed'").await, 1, "dry run writes nothing");

        let r = restore_metadata(pool, &backup, false, T).await.unwrap();
        assert_eq!((r.ignored_folders_added, r.projects_hidden), (1, 1));
        assert_eq!(count("SELECT COUNT(*) FROM projects WHERE folder_path = 'removed'").await, 0);
        assert_eq!(count("SELECT COUNT(*) FROM projects WHERE folder_path = 'kept'").await, 1);
        assert_eq!(count("SELECT COUNT(*) FROM ignored_folders").await, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn exports_one_project_with_only_what_it_uses() {
        let (db, dir) = db().await;
        let pool = db.pool();
        let dragon = indexed(pool, "dragon", &[("dragon.stl", "model", "h1")]).await;
        let bins = indexed(pool, "bins", &[("bin.stl", "model", "h3")]).await;
        let mut tx = pool.begin().await.unwrap();
        let pla = crate::queries::tags::ensure_tags(&mut tx, &["PLA".into()], T).await.unwrap();
        let petg = crate::queries::tags::ensure_tags(&mut tx, &["PETG".into()], T).await.unwrap();
        crate::queries::tags::set_project_tags(&mut tx, &dragon, &pla).await.unwrap();
        crate::queries::tags::set_project_tags(&mut tx, &bins, &petg).await.unwrap();
        tx.commit().await.unwrap();
        let desk = crate::queries::collections::create_collection(pool, "Desk", "", T).await.unwrap();
        crate::queries::collections::create_collection(pool, "Garage", "", T).await.unwrap();
        crate::queries::collections::add_project(pool, &desk.id, &dragon, T).await.unwrap();

        let b = export_project(pool, &dragon, T).await.unwrap().unwrap();
        assert_eq!(b.projects.len(), 1);
        assert_eq!(b.projects[0].folder_path, "dragon");
        assert_eq!(b.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["PLA"]);
        let names: Vec<_> = b.collections.iter().filter(|c| c.system_key.is_none()).map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Desk"]);
        assert!(export_project(pool, "missing", T).await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn synced_collections_and_hidden_duplicates_survive_a_rebuild() {
        let (old, dir1) = db().await;
        let pool = old.pool();
        let shark = indexed(pool, "shark", &[("shark.stl", "model", "s1")]).await;
        let copy = indexed(pool, "shark-copy", &[("shark.stl", "model", "s1")]).await;
        let toys = crate::queries::collections::create_collection(pool, "Toys", "", T).await.unwrap();
        crate::queries::collections::add_project(pool, &toys.id, &shark, T).await.unwrap();
        let src = crate::queries::collection_sources::SourceRow {
            id: Uuid::new_v4().to_string(),
            collection_id: toys.id.clone(),
            source: "makerworld".into(),
            source_id: "118240".into(),
            url: "https://makerworld.com/en/collections/118240".into(),
            keep_in_sync: true,
            last_synced_at: None,
            last_error: None,
            created_at: T.into(),
        };
        crate::queries::collection_sources::create(pool, &src).await.unwrap();
        let item = |model: &str, project: Option<&str>, imported, excluded| crate::queries::collection_sources::ItemRow {
            model_id: model.into(),
            model_url: format!("https://makerworld.com/en/models/{model}"),
            name: model.into(),
            project_id: project.map(String::from),
            imported,
            excluded,
            last_error: None,
        };
        crate::queries::collection_sources::upsert_item(pool, &src.id, &item("1", Some(&shark), true, false)).await.unwrap();
        crate::queries::collection_sources::upsert_item(pool, &src.id, &item("2", None, false, true)).await.unwrap();
        crate::queries::duplicates::dismiss(pool, &crate::queries::duplicates::project_group_key(&[shark.clone(), copy.clone()]), T).await.unwrap();
        crate::queries::duplicates::dismiss(pool, "file:abc", T).await.unwrap();

        let backup = export_metadata(pool, T).await.unwrap();
        assert_eq!(backup.collection_sources.len(), 1);
        assert_eq!(backup.collection_sources[0].items[0].project_folder.as_deref(), Some("shark"));
        assert_eq!(backup.duplicate_dismissals.len(), 2);

        // Rebuilt from disk: same folders, new ids, nothing else.
        let (new, dir2) = db().await;
        let pool = new.pool();
        let shark = indexed(pool, "shark", &[("shark.stl", "model", "s1")]).await;
        let copy = indexed(pool, "shark-copy", &[("shark.stl", "model", "s1")]).await;
        let r = restore_metadata(pool, &backup, false, T).await.unwrap();
        assert_eq!((r.collection_sources_added, r.duplicate_dismissals_added), (1, 2));

        let (source_ref, keep): (String, bool) = sqlx::query_as("SELECT id, keep_in_sync FROM collection_sources WHERE source_id = '118240'").fetch_one(pool).await.unwrap();
        assert!(keep);
        let items = crate::queries::collection_sources::items(pool, &source_ref).await.unwrap();
        let one = items.iter().find(|i| i.model_id == "1").unwrap();
        assert_eq!((one.project_id.as_deref(), one.imported), (Some(shark.as_str()), true), "points at the new project id");
        assert!(items.iter().find(|i| i.model_id == "2").unwrap().excluded);
        let report = crate::queries::duplicates::find(pool).await.unwrap();
        assert_eq!(report.dismissed, 1, "the group is hidden again under its new ids");
        assert!(report.project_groups.is_empty());
        let _ = copy;

        let again = restore_metadata(pool, &backup, false, T).await.unwrap();
        assert_eq!((again.collection_sources_added, again.duplicate_dismissals_added), (0, 0), "restoring twice adds nothing");
        let _ = std::fs::remove_dir_all(dir1);
        let _ = std::fs::remove_dir_all(dir2);
    }
}
