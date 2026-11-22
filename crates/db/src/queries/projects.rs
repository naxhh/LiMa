use sqlx::{Sqlite, Pool};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::Cursor;

/// Narrows project lists and searches. The default matches everything.
#[derive(Debug, Clone, Default)]
pub struct ProjectFilter {
    /// Projects must have every one of these tags.
    pub tag_ids: Vec<String>,
    /// Projects must have a model file with one of these extensions (upper case, no dot).
    pub formats: Vec<String>,
    /// Only projects without any image.
    pub no_image: bool,
}

impl ProjectFilter {
    pub fn is_empty(&self) -> bool {
        self.tag_ids.is_empty() && self.formats.is_empty() && !self.no_image
    }

    /// The three bind values used by [`FILTER_SQL`]: tag ids JSON, formats JSON, no-image flag.
    pub(crate) fn binds(&self) -> (String, String, bool) {
        (
            serde_json::to_string(&self.tag_ids).expect("serialize tags"),
            serde_json::to_string(&self.formats).expect("serialize formats"),
            self.no_image,
        )
    }
}

/// `AND …` clause over a `projects p` row, using bind parameters `?{t}`, `?{f}`, `?{n}`.
pub(crate) fn filter_sql(t: usize, f: usize, n: usize) -> String {
    format!(
        r#"
        AND (json_array_length(?{t}) = 0 OR (
            SELECT COUNT(DISTINCT pt.tag_id) FROM project_tags pt
            WHERE pt.project_id = p.id AND pt.tag_id IN (SELECT value FROM json_each(?{t}))
        ) = json_array_length(?{t}))
        AND (json_array_length(?{f}) = 0 OR EXISTS (
            SELECT 1 FROM assets a, json_each(?{f}) fmt
            WHERE a.project_id = p.id AND a.kind = 'model' AND a.file_path LIKE '%.' || fmt.value
        ))
        AND (?{n} = 0 OR NOT EXISTS (SELECT 1 FROM assets a WHERE a.project_id = p.id AND a.kind = 'image'))
        "#
    )
}

pub async fn list_projects(
    pool: &Pool<Sqlite>,
    limit: i64,
    cursor: Option<Cursor>,
    filter: &ProjectFilter,
) -> Result<Vec<ProjectRow>, sqlx::Error> {
    let (tags, formats, no_image) = filter.binds();
    let sql = format!(
        r#"
        SELECT p.id, p.folder_path, p.name, p.description, p.main_image_id, p.created_at, p.updated_at, p.last_scanned_at
        FROM projects p
        WHERE (?1 IS NULL OR (p.updated_at, p.id) < (?1, ?2))
        {filter}
        ORDER BY p.updated_at DESC, p.id DESC
        LIMIT ?3
        "#,
        filter = filter_sql(4, 5, 6)
    );
    let (updated_at, id) = match &cursor {
        Some(c) => (Some(c.updated_at.as_str()), Some(c.id.as_str())),
        None => (None, None),
    };
    sqlx::query_as::<_, ProjectRow>(&sql)
        .bind(updated_at)
        .bind(id)
        .bind(limit)
        .bind(tags)
        .bind(formats)
        .bind(no_image)
        .fetch_all(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use uuid::Uuid;

    const T: &str = "2026-01-01T00:00:00Z";

    async fn project(pool: &Pool<Sqlite>, name: &str, updated: &str, files: &[(&str, &str)], tags: &[&str]) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?4)")
            .bind(&id).bind(name).bind(T).bind(updated).execute(pool).await.unwrap();
        for (path, kind) in files {
            sqlx::query("INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, ?5)")
                .bind(Uuid::new_v4().to_string()).bind(&id).bind(path).bind(kind).bind(T).execute(pool).await.unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        let names: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let ids = crate::queries::tags::ensure_tags(&mut tx, &names, T).await.unwrap();
        crate::queries::tags::set_project_tags(&mut tx, &id, &ids).await.unwrap();
        tx.commit().await.unwrap();
        id
    }

    async fn tag_id(pool: &Pool<Sqlite>, name: &str) -> String {
        crate::queries::tags::find_tag_id_by_name(pool, name).await.unwrap().unwrap()
    }

    fn names(rows: Vec<ProjectRow>) -> Vec<String> {
        rows.into_iter().map(|p| p.name).collect()
    }

    #[tokio::test]
    async fn filters_by_tags_formats_and_missing_images() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        project(pool, "dragon", "2026-01-05T00:00:00Z", &[("dragon.STL", "model"), ("p.jpg", "image")], &["PLA", "gift"]).await;
        project(pool, "bins", "2026-01-04T00:00:00Z", &[("bins.3mf", "model")], &["PLA"]).await;
        project(pool, "duct", "2026-01-03T00:00:00Z", &[("duct.step", "model"), ("duct.stl", "model"), ("d.png", "image")], &[]).await;
        // A description mentioning a tag must not count as having it.
        let mention = project(pool, "mentions pla", "2026-01-02T00:00:00Z", &[("x.obj", "model")], &[]).await;
        sqlx::query("UPDATE projects SET description = 'print in PLA' WHERE id = ?1").bind(&mention).execute(pool).await.unwrap();

        let pla = tag_id(pool, "PLA").await;
        let gift = tag_id(pool, "gift").await;
        let all = |f: ProjectFilter| async move { names(list_projects(pool, 50, None, &f).await.unwrap()) };

        assert_eq!(all(ProjectFilter::default()).await, vec!["dragon", "bins", "duct", "mentions pla"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla.clone()], ..Default::default() }).await, vec!["dragon", "bins"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla.clone(), gift], ..Default::default() }).await, vec!["dragon"], "all tags must match");
        assert_eq!(all(ProjectFilter { formats: vec!["STL".into()], ..Default::default() }).await, vec!["dragon", "duct"], "case-insensitive");
        assert_eq!(all(ProjectFilter { formats: vec!["3MF".into(), "OBJ".into()], ..Default::default() }).await, vec!["bins", "mentions pla"], "any format");
        assert_eq!(all(ProjectFilter { no_image: true, ..Default::default() }).await, vec!["bins", "mentions pla"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla], no_image: true, ..Default::default() }).await, vec!["bins"]);

        // Pagination keeps working with a filter.
        let f = ProjectFilter { formats: vec!["STL".into(), "3MF".into()], ..Default::default() };
        let page1 = list_projects(pool, 2, None, &f).await.unwrap();
        let last = page1.last().unwrap();
        let cursor = Cursor { updated_at: last.updated_at.clone(), id: last.id.clone(), rank: None };
        assert_eq!(names(page1), vec!["dragon", "bins"]);
        assert_eq!(names(list_projects(pool, 2, Some(cursor), &f).await.unwrap()), vec!["duct"]);

        // Search combines with filters.
        let hits = crate::queries::projects_search::search_projects(pool, "d", 50, None, &ProjectFilter { formats: vec!["STEP".into()], ..Default::default() })
            .await
            .unwrap();
        assert_eq!(hits.into_iter().map(|h| h.project.name).collect::<Vec<_>>(), vec!["duct"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
