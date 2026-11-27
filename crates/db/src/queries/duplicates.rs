//! Duplicate detection (by content hash) and the "keep one" merge.
//!
//! - Duplicate projects: their model files are the same, or at least 80% the same.
//! - Shared files: the same file in projects that aren't duplicates of each other.
//!
//! Nothing here touches files: removing the other copies is done by the caller (trash or ignore).

use std::collections::{BTreeSet, HashMap};

use sqlx::{FromRow, Pool, Sqlite};

/// Two projects are duplicates when this share of the larger one's model files is identical.
const SIMILARITY: f64 = 0.8;
/// A file in more distinct model sets than this is treated as a common part (a calibration
/// cube, a shared base) and doesn't link projects on its own. Keeps pair counting cheap.
const MAX_SETS_PER_FILE: usize = 400;

#[derive(Debug, Clone)]
pub struct ProjectGroup {
    /// Stable id of the group: its sorted project ids.
    pub key: String,
    pub project_ids: Vec<String>,
    /// Model files every project in the group has.
    pub shared_models: i64,
    /// Whether all projects have exactly the same model files.
    pub identical: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct SharedFileEntry {
    pub project_id: String,
    pub asset_id: String,
    pub file_path: String,
    pub kind: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct SharedFile {
    pub key: String,
    pub hash: String,
    pub entries: Vec<SharedFileEntry>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub project_groups: Vec<ProjectGroup>,
    pub shared_files: Vec<SharedFile>,
    /// Files not hashed yet (a sync is still working on them); they can't be compared.
    pub unhashed_files: i64,
    pub dismissed: i64,
}

pub fn project_group_key(ids: &[String]) -> String {
    let mut ids = ids.to_vec();
    ids.sort();
    format!("projects:{}", ids.join(","))
}

fn file_key(hash: &str) -> String {
    format!("file:{hash}")
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut i = i;
        while self.0[i] != root {
            let next = self.0[i];
            self.0[i] = root;
            i = next;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// Groups projects by their model files. Input: (project id, model file hash).
fn group_projects(rows: Vec<(String, String)>) -> Vec<ProjectGroup> {
    let mut per_project: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (project, hash) in rows {
        per_project.entry(project).or_default().insert(hash);
    }
    // Projects with exactly the same model files share one set.
    let mut by_set: HashMap<Vec<String>, Vec<String>> = HashMap::new();
    for (project, hashes) in per_project {
        by_set.entry(hashes.into_iter().collect()).or_default().push(project);
    }
    let sets: Vec<(Vec<String>, Vec<String>)> = by_set.into_iter().collect();

    // Count the files each pair of sets has in common.
    let mut sets_with: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (hashes, _)) in sets.iter().enumerate() {
        for h in hashes {
            sets_with.entry(h.as_str()).or_default().push(i);
        }
    }
    let mut common: HashMap<(usize, usize), usize> = HashMap::new();
    for owners in sets_with.values().filter(|o| o.len() > 1 && o.len() <= MAX_SETS_PER_FILE) {
        for (n, &a) in owners.iter().enumerate() {
            for &b in &owners[n + 1..] {
                *common.entry((a, b)).or_default() += 1;
            }
        }
    }
    let mut uf = UnionFind((0..sets.len()).collect());
    for ((a, b), shared) in common {
        let larger = sets[a].0.len().max(sets[b].0.len());
        if shared as f64 / larger as f64 >= SIMILARITY {
            uf.union(a, b);
        }
    }

    let mut components: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..sets.len() {
        components.entry(uf.find(i)).or_default().push(i);
    }
    let mut groups: Vec<ProjectGroup> = components
        .into_values()
        .filter_map(|members| {
            let mut project_ids: Vec<String> = members.iter().flat_map(|&i| sets[i].1.iter().cloned()).collect();
            if project_ids.len() < 2 {
                return None;
            }
            project_ids.sort();
            let mut everywhere: BTreeSet<&String> = sets[members[0]].0.iter().collect();
            for &i in &members[1..] {
                let these: BTreeSet<&String> = sets[i].0.iter().collect();
                everywhere = everywhere.intersection(&these).copied().collect();
            }
            Some(ProjectGroup {
                key: project_group_key(&project_ids),
                shared_models: everywhere.len() as i64,
                identical: members.len() == 1,
                project_ids,
            })
        })
        .collect();
    groups.sort_by(|a, b| b.project_ids.len().cmp(&a.project_ids.len()).then_with(|| a.key.cmp(&b.key)));
    groups
}

pub async fn find(pool: &Pool<Sqlite>) -> Result<Report, sqlx::Error> {
    let dismissed: BTreeSet<String> = sqlx::query_scalar("SELECT key FROM duplicate_dismissals").fetch_all(pool).await?.into_iter().collect();

    let models: Vec<(String, String)> =
        sqlx::query_as("SELECT project_id, file_hash FROM assets WHERE kind = 'model' AND file_hash IS NOT NULL").fetch_all(pool).await?;
    let all_groups = group_projects(models);

    // Which group each project is in, so files shared inside a group aren't listed twice.
    let mut group_of: HashMap<&str, usize> = HashMap::new();
    for (i, g) in all_groups.iter().enumerate() {
        for id in &g.project_ids {
            group_of.insert(id.as_str(), i);
        }
    }

    let entries: Vec<(String, SharedFileEntry)> = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
        r#"
        SELECT file_hash, project_id, id, file_path, kind, size_bytes FROM assets
        WHERE size_bytes > 0 AND file_hash IN (
            SELECT file_hash FROM assets WHERE file_hash IS NOT NULL AND size_bytes > 0
            GROUP BY file_hash HAVING COUNT(DISTINCT project_id) > 1
        )
        ORDER BY file_hash, project_id, file_path
        "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(hash, project_id, asset_id, file_path, kind, size_bytes)| (hash, SharedFileEntry { project_id, asset_id, file_path, kind, size_bytes }))
    .collect();
    let mut by_hash: Vec<SharedFile> = Vec::new();
    for (hash, entry) in entries {
        match by_hash.last_mut() {
            Some(f) if f.hash == hash => f.entries.push(entry),
            _ => by_hash.push(SharedFile { key: file_key(&hash), hash, entries: vec![entry] }),
        }
    }
    let mut shared_files: Vec<SharedFile> = by_hash
        .into_iter()
        .filter(|f| {
            // Listed only when it spans projects that aren't one duplicate group.
            let groups: BTreeSet<Option<usize>> = f.entries.iter().map(|e| group_of.get(e.project_id.as_str()).copied()).collect();
            groups.len() > 1 || groups.contains(&None)
        })
        .collect();
    shared_files.sort_by_key(|f| std::cmp::Reverse(f.entries.iter().map(|e| e.size_bytes).sum::<i64>()));

    let unhashed_files = sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE file_hash IS NULL").fetch_one(pool).await?;
    let mut hidden = 0;
    let project_groups = all_groups
        .into_iter()
        .filter(|g| {
            let keep = !dismissed.contains(&g.key);
            hidden += i64::from(!keep);
            keep
        })
        .collect();
    let shared_files = shared_files
        .into_iter()
        .filter(|f| {
            let keep = !dismissed.contains(&f.key);
            hidden += i64::from(!keep);
            keep
        })
        .collect();
    Ok(Report { project_groups, shared_files, unhashed_files, dismissed: hidden })
}

/// A project already in the library that the model being imported looks like.
#[derive(Debug, Clone)]
pub struct SimilarProject {
    pub project_id: String,
    pub name: String,
    /// Asset to show as its thumbnail: the cover, or a model when it has no image.
    pub cover_asset_id: Option<String>,
}

/// The project to link to when an import's model files already exist: the one sharing
/// at least [`SIMILARITY`] of the larger file set. `hashes` are the imported model files.
pub async fn find_similar(pool: &Pool<Sqlite>, hashes: &[String]) -> Result<Option<SimilarProject>, sqlx::Error> {
    if hashes.is_empty() {
        return Ok(None);
    }
    let json = serde_json::to_string(hashes).expect("serialize hashes");
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT p.id, p.name, p.main_image_id
        FROM assets a
        JOIN projects p ON p.id = a.project_id
        WHERE a.kind = 'model' AND a.file_hash IN (SELECT value FROM json_each(?1))
        GROUP BY p.id
        HAVING CAST(COUNT(DISTINCT a.file_hash) AS REAL) / MAX(
                 CAST(?2 AS INTEGER),
                 (SELECT COUNT(DISTINCT x.file_hash) FROM assets x
                  WHERE x.project_id = p.id AND x.kind = 'model' AND x.file_hash IS NOT NULL)
               ) >= ?3
        ORDER BY COUNT(DISTINCT a.file_hash) DESC
        LIMIT 1
        "#,
    )
    .bind(&json)
    .bind(hashes.len() as i64)
    .bind(SIMILARITY)
    .fetch_optional(pool)
    .await?;
    let Some((project_id, name, main_image_id)) = row else { return Ok(None) };
    let cover_asset_id = cover_of(pool, &project_id, main_image_id).await?;
    Ok(Some(SimilarProject { project_id, name, cover_asset_id }))
}

/// What to link to for a project the library already has for this model page.
pub async fn describe(pool: &Pool<Sqlite>, project_id: &str) -> Result<Option<SimilarProject>, sqlx::Error> {
    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, main_image_id FROM projects WHERE id = ?1").bind(project_id).fetch_optional(pool).await?;
    let Some((project_id, name, main_image_id)) = row else { return Ok(None) };
    let cover_asset_id = cover_of(pool, &project_id, main_image_id).await?;
    Ok(Some(SimilarProject { project_id, name, cover_asset_id }))
}

async fn cover_of(pool: &Pool<Sqlite>, project_id: &str, main_image_id: Option<String>) -> Result<Option<String>, sqlx::Error> {
    if main_image_id.is_some() {
        return Ok(main_image_id);
    }
    // No image: a model's preview thumbnail stands in for it the way the cards do.
    sqlx::query_scalar("SELECT id FROM assets WHERE project_id = ?1 AND kind = 'model' ORDER BY file_path LIMIT 1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
}

pub async fn dismiss(pool: &Pool<Sqlite>, key: &str, now: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO duplicate_dismissals (key, created_at) VALUES (?1, ?2)").bind(key).bind(now).execute(pool).await?;
    Ok(())
}

/// Shows every dismissed group again. Returns how many there were.
pub async fn clear_dismissed(pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM duplicate_dismissals").execute(pool).await?.rows_affected())
}

#[derive(Debug, Default, PartialEq)]
pub struct Merged {
    pub tags: u64,
    pub collections: u64,
    pub description: bool,
    pub source: bool,
    pub cover: bool,
}

/// Copies what the user curated on `others` onto `keep`: tags and collections (Favourites
/// included) are combined; description, source and cover fill in only where `keep` has none.
/// Returns `None` when `keep` doesn't exist.
pub async fn merge_into(pool: &Pool<Sqlite>, keep: &str, others: &[String], now: &str) -> Result<Option<Merged>, sqlx::Error> {
    let others_json = serde_json::to_string(others).expect("serialize ids");
    let mut tx = pool.begin().await?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE id = ?1").bind(keep).fetch_one(&mut *tx).await?;
    if exists == 0 {
        return Ok(None);
    }
    let tags = sqlx::query(
        "INSERT OR IGNORE INTO project_tags (project_id, tag_id)
         SELECT DISTINCT ?1, tag_id FROM project_tags WHERE project_id IN (SELECT value FROM json_each(?2))",
    )
    .bind(keep)
    .bind(&others_json)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // Keeps the earliest "added" date, so collection order sorted by date stays meaningful.
    let added = sqlx::query(
        "INSERT OR IGNORE INTO collection_projects (collection_id, project_id, created_at)
         SELECT collection_id, ?1, MIN(created_at) FROM collection_projects
         WHERE project_id IN (SELECT value FROM json_each(?2)) GROUP BY collection_id",
    )
    .bind(keep)
    .bind(&others_json)
    .execute(&mut *tx)
    .await?;
    let collections = added.rows_affected();
    if collections > 0 {
        sqlx::query(
            "UPDATE collections SET updated_at = ?2
             WHERE id IN (SELECT collection_id FROM collection_projects WHERE project_id IN (SELECT value FROM json_each(?1)))",
        )
        .bind(&others_json)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let description = sqlx::query(
        "UPDATE projects SET description = (
             SELECT description FROM projects WHERE id IN (SELECT value FROM json_each(?2)) AND trim(description) <> ''
             ORDER BY length(description) DESC LIMIT 1)
         WHERE id = ?1 AND trim(description) = ''
           AND EXISTS (SELECT 1 FROM projects WHERE id IN (SELECT value FROM json_each(?2)) AND trim(description) <> '')",
    )
    .bind(keep)
    .bind(&others_json)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;

    let source = sqlx::query(
        "UPDATE projects SET (source_url, source_author, source_license) = (
             SELECT source_url, source_author, source_license FROM projects
             WHERE id IN (SELECT value FROM json_each(?2)) AND source_url IS NOT NULL ORDER BY updated_at DESC LIMIT 1)
         WHERE id = ?1 AND source_url IS NULL
           AND EXISTS (SELECT 1 FROM projects WHERE id IN (SELECT value FROM json_each(?2)) AND source_url IS NOT NULL)",
    )
    .bind(keep)
    .bind(&others_json)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;

    // The other copy's cover goes away with it, so use the same picture from the kept project's own files.
    let cover = sqlx::query(
        "UPDATE projects SET main_image_id = (
             SELECT mine.id FROM projects o
             JOIN assets theirs ON theirs.id = o.main_image_id
             JOIN assets mine ON mine.project_id = ?1 AND mine.file_hash = theirs.file_hash
             WHERE o.id IN (SELECT value FROM json_each(?2)) LIMIT 1)
         WHERE id = ?1 AND main_image_id IS NULL
           AND EXISTS (
             SELECT 1 FROM projects o
             JOIN assets theirs ON theirs.id = o.main_image_id
             JOIN assets mine ON mine.project_id = ?1 AND mine.file_hash = theirs.file_hash
             WHERE o.id IN (SELECT value FROM json_each(?2)))",
    )
    .bind(keep)
    .bind(&others_json)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;

    let merged = Merged {
        tags,
        collections,
        description,
        source,
        cover,
    };

    if merged != Merged::default() {
        sqlx::query("UPDATE projects SET updated_at = ?2 WHERE id = ?1").bind(keep).bind(now).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(merged))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use uuid::Uuid;

    fn rows(spec: &[(&str, &[&str])]) -> Vec<(String, String)> {
        spec.iter().flat_map(|(p, hs)| hs.iter().map(move |h| (p.to_string(), h.to_string()))).collect()
    }

    #[test]
    fn groups_identical_and_nearly_identical_projects() {
        let groups = group_projects(rows(&[
            ("a", &["1", "2", "3", "4", "5"]),
            ("b", &["1", "2", "3", "4", "5"]),
            ("c", &["1", "2", "3", "4", "9"]), // 4 of 5 shared: 80%
            ("d", &["1", "7", "8"]),           // shares one file only
            ("e", &["x"]),
            ("f", &["x"]),
        ]));
        let ids: Vec<Vec<&str>> = groups.iter().map(|g| g.project_ids.iter().map(String::as_str).collect()).collect();
        assert_eq!(ids, vec![vec!["a", "b", "c"], vec!["e", "f"]]);
        assert!(!groups[0].identical);
        assert_eq!(groups[0].shared_models, 4);
        assert!(groups[1].identical);
    }

    async fn project(pool: &Pool<Sqlite>, name: &str, description: &str) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, description, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, 't', 't')")
            .bind(&id)
            .bind(name)
            .bind(description)
            .execute(pool)
            .await
            .unwrap();
        id
    }

    async fn asset(pool: &Pool<Sqlite>, project: &str, path: &str, kind: &str, hash: &str) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, file_hash, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 10, 't', ?5, 't', 't')",
        )
        .bind(&id)
        .bind(project)
        .bind(path)
        .bind(kind)
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn finds_merges_and_dismisses() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        let keep = project(pool, "benchy", "").await;
        let copy = project(pool, "benchy copy", "A tiny boat").await;
        let other = project(pool, "boat stand", "").await;
        asset(pool, &keep, "benchy.stl", "model", "h1").await;
        let my_photo = asset(pool, &keep, "photo.jpg", "image", "p1").await;
        asset(pool, &copy, "benchy.stl", "model", "h1").await;
        let their_photo = asset(pool, &copy, "photo.jpg", "image", "p1").await;
        asset(pool, &other, "stand.stl", "model", "s1").await;
        asset(pool, &other, "benchy.stl", "model", "h1").await;
        sqlx::query("UPDATE projects SET main_image_id = ?2, source_url = 'https://example.com/b' WHERE id = ?1")
            .bind(&copy)
            .bind(&their_photo)
            .execute(pool)
            .await
            .unwrap();

        let report = find(pool).await.unwrap();
        assert_eq!(report.project_groups.len(), 1);
        let mut expected = vec![keep.clone(), copy.clone()];
        expected.sort();
        assert_eq!(report.project_groups[0].project_ids, expected);
        assert_eq!(report.shared_files.len(), 1, "benchy.stl is also in a project that isn't a duplicate");
        assert_eq!(report.shared_files[0].entries.len(), 3);

        crate::queries::bulk::add_tags(pool, std::slice::from_ref(&copy), &["boat".into()], "t").await.unwrap();
        let fav = crate::queries::collections::ensure_favourites(pool, "t").await.unwrap();
        crate::queries::collections::add_project(pool, &fav, &copy, "t").await.unwrap();

        let merged = merge_into(pool, &keep, std::slice::from_ref(&copy), "t2").await.unwrap().unwrap();
        assert_eq!(merged, Merged { tags: 1, collections: 1, description: true, source: true, cover: true });
        let (description, cover, source): (String, Option<String>, Option<String>) =
            sqlx::query_as("SELECT description, main_image_id, source_url FROM projects WHERE id = ?1").bind(&keep).fetch_one(pool).await.unwrap();
        assert_eq!(description, "A tiny boat");
        assert_eq!(cover.as_deref(), Some(my_photo.as_str()), "the kept project's own copy of the cover");
        assert_eq!(source.as_deref(), Some("https://example.com/b"));
        assert!(crate::queries::collections::is_favourite(pool, &keep).await.unwrap());
        assert_eq!(merge_into(pool, &keep, std::slice::from_ref(&copy), "t3").await.unwrap().unwrap(), Merged::default(), "nothing new the second time");
        assert_eq!(merge_into(pool, "missing", std::slice::from_ref(&copy), "t").await.unwrap(), None);

        dismiss(pool, &report.project_groups[0].key, "t").await.unwrap();
        let report = find(pool).await.unwrap();
        assert!(report.project_groups.is_empty());
        assert_eq!(report.dismissed, 1);
        assert_eq!(clear_dismissed(pool).await.unwrap(), 1);
        assert_eq!(find(pool).await.unwrap().project_groups.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn finds_a_similar_project_by_hashes() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        let project_id = project(pool, "benchy", "").await;
        asset(pool, &project_id, "benchy.stl", "model", "h1").await;
        asset(pool, &project_id, "hull.stl", "model", "h2").await;
        asset(pool, &project_id, "photo.jpg", "image", "p1").await;

        let found = find_similar(pool, &["h1".into(), "h2".into()]).await.unwrap().unwrap();
        assert_eq!(found.project_id, project_id);
        assert_eq!(found.name, "benchy");
        assert!(found.cover_asset_id.is_some(), "a model stands in when there is no image");

        assert!(find_similar(pool, &["h1".into()]).await.unwrap().is_none(), "1 of 2 files is only 50%");
        assert!(find_similar(pool, &[]).await.unwrap().is_none());

        let described = describe(pool, &project_id).await.unwrap().unwrap();
        assert_eq!(described.project_id, project_id);
        assert!(describe(pool, "missing").await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
