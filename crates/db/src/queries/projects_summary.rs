//! Per-project aggregates for list views (file counts, size, formats, tags)
//! and library-wide totals.

use std::collections::{BTreeSet, HashMap};

use sqlx::{FromRow, Pool, Sqlite};

#[derive(Debug, Default, Clone)]
pub struct ProjectSummary {
    pub asset_count: i64,
    pub image_count: i64,
    pub model_count: i64,
    pub total_bytes: i64,
    /// Upper-case extensions of the project's model files, sorted (e.g. `["3MF", "STL"]`).
    pub model_formats: Vec<String>,
    pub tags: Vec<SummaryTag>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SummaryTag {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(FromRow)]
struct CountRow {
    project_id: String,
    asset_count: i64,
    image_count: i64,
    model_count: i64,
    total_bytes: i64,
}

#[derive(FromRow)]
struct ModelPathRow {
    project_id: String,
    file_path: String,
}

#[derive(FromRow)]
struct TagLinkRow {
    project_id: String,
    id: String,
    name: String,
    color: String,
}

/// Summaries for the given projects. Projects without assets or tags get an empty summary.
pub async fn project_summaries(
    pool: &Pool<Sqlite>,
    project_ids: &[String],
) -> Result<HashMap<String, ProjectSummary>, sqlx::Error> {
    let mut out: HashMap<String, ProjectSummary> =
        project_ids.iter().map(|id| (id.clone(), ProjectSummary::default())).collect();
    if project_ids.is_empty() {
        return Ok(out);
    }
    // One bound JSON array instead of a variable-length IN list.
    let ids_json = serde_json::to_string(project_ids).expect("serialize ids");

    let counts = sqlx::query_as::<_, CountRow>(
        r#"
        SELECT project_id,
               COUNT(*) AS asset_count,
               SUM(kind = 'image') AS image_count,
               SUM(kind = 'model') AS model_count,
               COALESCE(SUM(size_bytes), 0) AS total_bytes
        FROM assets
        WHERE project_id IN (SELECT value FROM json_each(?1))
        GROUP BY project_id
        "#,
    )
    .bind(&ids_json)
    .fetch_all(pool)
    .await?;
    for c in counts {
        if let Some(s) = out.get_mut(&c.project_id) {
            s.asset_count = c.asset_count;
            s.image_count = c.image_count;
            s.model_count = c.model_count;
            s.total_bytes = c.total_bytes;
        }
    }

    let models = sqlx::query_as::<_, ModelPathRow>(
        r#"
        SELECT project_id, file_path
        FROM assets
        WHERE kind = 'model' AND project_id IN (SELECT value FROM json_each(?1))
        "#,
    )
    .bind(&ids_json)
    .fetch_all(pool)
    .await?;
    let mut formats: HashMap<String, BTreeSet<String>> = HashMap::new();
    for m in models {
        if let Some(ext) = std::path::Path::new(&m.file_path).extension().and_then(|e| e.to_str()) {
            formats.entry(m.project_id).or_default().insert(ext.to_ascii_uppercase());
        }
    }
    for (id, set) in formats {
        if let Some(s) = out.get_mut(&id) {
            s.model_formats = set.into_iter().collect();
        }
    }

    let tags = sqlx::query_as::<_, TagLinkRow>(
        r#"
        SELECT pt.project_id, t.id, t.name, t.color
        FROM project_tags pt
        JOIN tags t ON t.id = pt.tag_id
        WHERE pt.project_id IN (SELECT value FROM json_each(?1))
        ORDER BY t.name COLLATE NOCASE
        "#,
    )
    .bind(&ids_json)
    .fetch_all(pool)
    .await?;
    for t in tags {
        if let Some(s) = out.get_mut(&t.project_id) {
            s.tags.push(SummaryTag { id: t.id, name: t.name, color: t.color });
        }
    }

    Ok(out)
}

#[derive(Debug, FromRow)]
pub struct LibraryStatsRow {
    pub projects: i64,
    pub assets: i64,
    pub total_bytes: i64,
}

pub async fn library_stats(pool: &Pool<Sqlite>) -> Result<LibraryStatsRow, sqlx::Error> {
    sqlx::query_as::<_, LibraryStatsRow>(
        r#"
        SELECT (SELECT COUNT(*) FROM projects) AS projects,
               (SELECT COUNT(*) FROM assets) AS assets,
               (SELECT COALESCE(SUM(size_bytes), 0) FROM assets) AS total_bytes
        "#,
    )
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use uuid::Uuid;

    const NOW: &str = "2026-01-01T00:00:00Z";

    #[tokio::test]
    async fn summaries_count_assets_formats_and_tags() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        let (a, b) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
        for (id, name) in [(&a, "a"), (&b, "b")] {
            sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?3)")
                .bind(id).bind(name).bind(NOW).execute(pool).await.unwrap();
        }
        for (path, kind, size) in [("x.stl", "model", 10), ("y.3mf", "model", 20), ("z.STL", "model", 5), ("p.jpg", "image", 7)] {
            sqlx::query("INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6)")
                .bind(Uuid::new_v4().to_string()).bind(&a).bind(path).bind(kind).bind(size).bind(NOW)
                .execute(pool).await.unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        let ids = crate::queries::tags::ensure_tags(&mut tx, &["PLA".to_string(), "desk".to_string()], NOW).await.unwrap();
        crate::queries::tags::set_project_tags(&mut tx, &a, &ids).await.unwrap();
        tx.commit().await.unwrap();

        let s = project_summaries(pool, &[a.clone(), b.clone()]).await.unwrap();
        let sa = &s[&a];
        assert_eq!((sa.asset_count, sa.model_count, sa.image_count, sa.total_bytes), (4, 3, 1, 42));
        assert_eq!(sa.model_formats, vec!["3MF", "STL"]);
        assert_eq!(sa.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["desk", "PLA"]);
        assert_eq!(s[&b].asset_count, 0);
        assert!(s[&b].tags.is_empty());

        let stats = library_stats(pool).await.unwrap();
        assert_eq!((stats.projects, stats.assets, stats.total_bytes), (2, 4, 42));
        let _ = std::fs::remove_dir_all(dir);
    }
}
