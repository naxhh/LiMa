use sqlx::{Sqlite, Pool};
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectSort { #[default] Updated, Created, Name, Size, Files }

impl ProjectSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectSort::Updated => "updated",
            ProjectSort::Created => "created",
            ProjectSort::Name => "name",
            ProjectSort::Size => "size",
            ProjectSort::Files => "files",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "updated" => Some(ProjectSort::Updated),
            "created" => Some(ProjectSort::Created),
            "name" => Some(ProjectSort::Name),
            "size" => Some(ProjectSort::Size),
            "files" => Some(ProjectSort::Files),
            _ => None,
        }
    }

    fn sql_expr(&self) -> &'static str {
        match self {
            ProjectSort::Updated => "p.updated_at",
            ProjectSort::Created => "p.created_at",
            ProjectSort::Name => "lower(p.name)",
            ProjectSort::Size => "(SELECT COALESCE(SUM(a.size_bytes), 0) FROM assets a WHERE a.project_id = p.id)",
            ProjectSort::Files => "(SELECT COUNT(*) FROM assets a WHERE a.project_id = p.id)",
        }
    }
}

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
    cursor_value: Option<(serde_json::Value, String)>,
    sort: ProjectSort,
    descending: bool,
    filter: &ProjectFilter,
) -> Result<Vec<(ProjectRow, serde_json::Value)>, sqlx::Error> {
    let (tags, formats, no_image) = filter.binds();
    
    // Determine the comparison operator and direction based on descending flag
    let cmp = if descending { "<" } else { ">" };
    let dir = if descending { "DESC" } else { "ASC" };
    
    // Get the SQL expression for sorting
    let expr = sort.sql_expr();
    
    let sql = format!(
        r#"
        WITH s AS (
            SELECT p.id, p.folder_path, p.name, p.description, p.main_image_id, p.created_at, p.updated_at, p.last_scanned_at,
                   {expr} AS sort_value
            FROM projects p
            WHERE 1 = 1 {filter}
        )
        SELECT * FROM s
        WHERE (?1 IS NULL OR (sort_value, id) {cmp} (?1, ?2))
        ORDER BY sort_value {dir}, id {dir}
        LIMIT ?3
        "#,
        filter = filter_sql(4, 5, 6),
        expr = expr,
        cmp = cmp,
        dir = dir
    );
    
    let (sort_value, id) = match &cursor_value {
        Some((value, id)) => {
            // Bind based on the sort type - i64 for Size/Files, string otherwise
            (Some(value), Some(id))
        },
        None => (None, None),
    };
    
    // Create a small struct to hold the project row plus sort value
    #[derive(sqlx::FromRow)]
    struct ProjectWithSortValue {
        id: String,
        folder_path: String,
        name: String,
        description: String,
        main_image_id: Option<String>,
        created_at: String,
        updated_at: String,
        last_scanned_at: Option<String>,
        sort_value: serde_json::Value,
    }
    
    let mut query = sqlx::query_as::<_, ProjectWithSortValue>(&sql);
    
    // Bind parameters - handle different types based on sort
    if let Some((value, id)) = &cursor_value {
        match sort {
            ProjectSort::Size | ProjectSort::Files => {
                // For Size and Files, bind as i64
                query = query.bind(value.as_i64()).bind(id);
            }
            _ => {
                // For other sorts, bind as string
                query = query.bind(value.as_str()).bind(id);
            }
        }
    } else {
        query = query.bind(None::<i64>).bind(None::<String>);
    }
    
    query = query
        .bind(limit)
        .bind(tags)
        .bind(formats)
        .bind(no_image);

    let rows = query.fetch_all(pool).await?;
    
    // Convert to the expected return type (ProjectRow, sort_value)
    let result: Vec<(ProjectRow, serde_json::Value)> = rows.into_iter()
        .map(|row| {
            let project_row = ProjectRow {
                id: row.id,
                folder_path: row.folder_path,
                name: row.name,
                description: row.description,
                main_image_id: row.main_image_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                last_scanned_at: row.last_scanned_at,
            };
            (project_row, row.sort_value)
        })
        .collect();
    
    Ok(result)
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
        let all = |f: ProjectFilter| async move { names(list_projects(pool, 50, None, ProjectSort::Updated, true, &f).await.unwrap()) };

        assert_eq!(all(ProjectFilter::default()).await, vec!["dragon", "bins", "duct", "mentions pla"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla.clone()], ..Default::default() }).await, vec!["dragon", "bins"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla.clone(), gift], ..Default::default() }).await, vec!["dragon"], "all tags must match");
        assert_eq!(all(ProjectFilter { formats: vec!["STL".into()], ..Default::default() }).await, vec!["dragon", "duct"], "case-insensitive");
        assert_eq!(all(ProjectFilter { formats: vec!["3MF".into(), "OBJ".into()], ..Default::default() }).await, vec!["bins", "mentions pla"], "any format");
        assert_eq!(all(ProjectFilter { no_image: true, ..Default::default() }).await, vec!["bins", "mentions pla"]);
        assert_eq!(all(ProjectFilter { tag_ids: vec![pla], no_image: true, ..Default::default() }).await, vec!["bins"]);

        // Pagination keeps working with a filter.
        let f = ProjectFilter { formats: vec!["STL".into(), "3MF".into()], ..Default::default() };
        let page1 = list_projects(pool, 2, None, ProjectSort::Updated, true, &f).await.unwrap();
        let last = page1.last().unwrap();
        let cursor = Cursor { updated_at: last.0.updated_at.clone(), id: last.0.id.clone(), rank: None, sort: None, sort_value: None };
        assert_eq!(names(page1.into_iter().map(|p| p.0).collect()), vec!["dragon", "bins"]);
        let page2 = list_projects(pool, 2, Some((last.1, last.0.id.clone())), ProjectSort::Updated, true, &f).await.unwrap();
        assert_eq!(names(page2.into_iter().map(|p| p.0).collect()), vec!["duct"]);

        // Search combines with filters.
        let hits = crate::queries::projects_search::search_projects(pool, "d", 50, None, &ProjectFilter { formats: vec!["STEP".into()], ..Default::default() })
            .await
            .unwrap();
        assert_eq!(hits.into_iter().map(|h| h.project.name).collect::<Vec<_>>(), vec!["duct"]);
        let _ = std::fs::remove_dir_all(dir);
    }
    
    #[tokio::test]
    async fn sorts_and_paginates() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-sort-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        // Create 5 projects with different names and file counts
        project(pool, "Alpha Project", "2026-01-01T00:00:00Z", &[("file1.stl", "model")], &[]).await;
        project(pool, "Beta Project", "2026-01-02T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model")], &[]).await;
        project(pool, "Gamma Project", "2026-01-03T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model"), ("file3.step", "model")], &[]).await;
        project(pool, "Delta Project", "2026-01-04T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model"), ("file3.step", "model"), ("file4.obj", "model")], &[]).await;
        project(pool, "Epsilon Project", "2026-01-05T00:00:00Z", &[("file1.stl", "model")], &[]).await;

        // Test sorting by name (ascending and descending)
        let names_asc = list_projects(pool, 50, None, ProjectSort::Name, false, &ProjectFilter::default()).await.unwrap();
        let names_asc = names_asc.into_iter().map(|(p, _)| p.name).collect::<Vec<_>>();
        
        assert_eq!(names_asc, vec!["Alpha Project", "Beta Project", "Delta Project", "Epsilon Project", "Gamma Project"]);
        // For descending order we'll need to test with a different approach since we're not using closures

        // Test pagination for name sort
        let page1 = list_projects(pool, 2, None, ProjectSort::Name, false, &ProjectFilter::default()).await.unwrap();
        let last1 = page1.last().unwrap();
        let page2 = list_projects(pool, 2, Some((last1.1.clone(), last1.0.id.clone())), ProjectSort::Name, false, &ProjectFilter::default()).await.unwrap();
        let page3 = list_projects(pool, 2, Some((page2.last().unwrap().1.clone(), page2.last().unwrap().0.id.clone())), ProjectSort::Name, false, &ProjectFilter::default()).await.unwrap();
        
        // Combine pages and check for duplicates or gaps
        let all_pages: Vec<String> = page1.iter().chain(page2.iter()).chain(page3.iter())
            .map(|(p, _)| p.name.clone())
            .collect();
        assert_eq!(all_pages.len(), 5);
        assert_eq!(all_pages, vec!["Alpha Project", "Beta Project", "Delta Project", "Epsilon Project", "Gamma Project"]);
        
        // Test sorting by size (create projects with different file counts)
        let dir2 = std::env::temp_dir().join(format!("lima-db-test-size-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir2).unwrap();
        let db2 = Db::connect(&format!("sqlite:{}?mode=rwc", dir2.join("t.db").display())).await.unwrap();
        db2.migrate().await.unwrap();
        let pool2 = db2.pool();

        // Create projects with different file counts to test size sorting
        project(pool2, "Small Project", "2026-01-01T00:00:00Z", &[("file1.stl", "model")], &[]).await;  // 1 file
        project(pool2, "Medium Project", "2026-01-02T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model")], &[]).await; // 2 files
        project(pool2, "Large Project", "2026-01-03T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model"), ("file3.step", "model")], &[]).await; // 3 files
        project(pool2, "Another Large Project", "2026-01-04T00:00:00Z", &[("file1.stl", "model"), ("file2.3mf", "model"), ("file3.step", "model")], &[]).await; // 3 files (same count)
        
        let mut sizes = |sort, desc| {
            list_projects(pool2, 50, None, sort, desc, &ProjectFilter::default())
                .await
                .unwrap()
                .into_iter()
                .map(|(p, _)| p.name)
                .collect::<Vec<_>>()
        };
        
        // Test ascending size order (small to large)
        assert_eq!(sizes(ProjectSort::Files, false), vec!["Small Project", "Medium Project", "Another Large Project", "Large Project"]);
        
        // Test descending size order (large to small)  
        assert_eq!(sizes(ProjectSort::Files, true), vec!["Large Project", "Another Large Project", "Medium Project", "Small Project"]);
        
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }
}
