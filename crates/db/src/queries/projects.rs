use sqlx::{FromRow, Pool, Row, Sqlite};
use lima_domain::models::project::ProjectRow;

/// Library list order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectSort { #[default] Updated, Created, Modified, Name, Size, Files }

impl ProjectSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectSort::Updated => "updated",
            ProjectSort::Created => "created",
            ProjectSort::Modified => "modified",
            ProjectSort::Name => "name",
            ProjectSort::Size => "size",
            ProjectSort::Files => "files",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "updated" => Some(ProjectSort::Updated),
            "created" => Some(ProjectSort::Created),
            "modified" => Some(ProjectSort::Modified),
            "name" => Some(ProjectSort::Name),
            "size" => Some(ProjectSort::Size),
            "files" => Some(ProjectSort::Files),
            _ => None,
        }
    }

    /// Size and file count sort by number; the rest by text.
    fn is_numeric(&self) -> bool {
        matches!(self, ProjectSort::Size | ProjectSort::Files)
    }

    /// SQL for the sort value of a `projects p` row. Never NULL, so keyset comparisons work.
    pub(crate) fn sql_expr(&self) -> &'static str {
        match self {
            ProjectSort::Updated => "p.updated_at",
            ProjectSort::Created => "p.created_at",
            // Newest file date on disk; projects without files sort as oldest.
            ProjectSort::Modified => "COALESCE((SELECT MAX(a.mtime) FROM assets a WHERE a.project_id = p.id), '')",
            ProjectSort::Name => "lower(p.name)",
            ProjectSort::Size => "(SELECT COALESCE(SUM(a.size_bytes), 0) FROM assets a WHERE a.project_id = p.id)",
            ProjectSort::Files => "(SELECT COUNT(*) FROM assets a WHERE a.project_id = p.id)",
        }
    }
}

/// Narrows project lists and searches. The default matches everything. Names and text are
/// compared in lower case (the search parser lowercases them).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProjectFilter {
    /// Projects must have every one of these tags.
    pub tag_ids: Vec<String>,
    /// Projects must have a model file with one of these extensions (upper case, no dot).
    pub formats: Vec<String>,
    /// Only projects without any image.
    pub no_image: bool,
    /// Tags by name (`#pla`): all must be on the project.
    pub tag_names: Vec<String>,
    /// Tags by name the project must not have (`-#gift`).
    pub not_tag_names: Vec<String>,
    /// Collections by name (`in:desk`; `favourites` matches the built-in one): all must hold it.
    pub collections: Vec<String>,
    pub not_collections: Vec<String>,
    /// In Favourites (`is:fav`) or not (`-is:fav`).
    pub favourite: Option<bool>,
    /// Has a picture (`has:image`) or not (`no:image`).
    pub has_image: Option<bool>,
    /// Imported from one of these hosts (`from:printables`).
    pub sources: Vec<String>,
    /// The source author contains each of these (`by:prusa`).
    pub authors: Vec<String>,
    /// A file path contains each of these (`file:benchy`).
    pub files: Vec<String>,
}

impl ProjectFilter {
    /// The single JSON bind value used by [`filter_sql`].
    pub(crate) fn bind_json(&self) -> String {
        serde_json::to_string(self).expect("serialize filter")
    }
}

/// `AND …` clause over a `projects p` row; every filter comes from one JSON bind parameter `?{j}`.
pub(crate) fn filter_sql(j: usize) -> String {
    format!(
        r#"
        AND (json_array_length(?{j}, '$.tag_ids') = 0 OR (
            SELECT COUNT(DISTINCT pt.tag_id) FROM project_tags pt
            WHERE pt.project_id = p.id AND pt.tag_id IN (SELECT value FROM json_each(?{j}, '$.tag_ids'))
        ) = json_array_length(?{j}, '$.tag_ids'))
        AND (json_array_length(?{j}, '$.formats') = 0 OR EXISTS (
            SELECT 1 FROM assets a, json_each(?{j}, '$.formats') fmt
            WHERE a.project_id = p.id AND a.kind = 'model' AND a.file_path LIKE '%.' || fmt.value
        ))
        AND (json_extract(?{j}, '$.no_image') = 0 OR NOT EXISTS (SELECT 1 FROM assets a WHERE a.project_id = p.id AND a.kind = 'image'))
        AND (json_extract(?{j}, '$.has_image') IS NULL
             OR json_extract(?{j}, '$.has_image') = EXISTS (SELECT 1 FROM assets a WHERE a.project_id = p.id AND a.kind = 'image'))
        AND NOT EXISTS (
            SELECT 1 FROM json_each(?{j}, '$.tag_names') n WHERE NOT EXISTS (
                SELECT 1 FROM project_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.project_id = p.id AND lower(t.name) = n.value))
        AND NOT EXISTS (
            SELECT 1 FROM project_tags pt JOIN tags t ON t.id = pt.tag_id, json_each(?{j}, '$.not_tag_names') n
            WHERE pt.project_id = p.id AND lower(t.name) = n.value)
        AND NOT EXISTS (
            SELECT 1 FROM json_each(?{j}, '$.collections') n WHERE NOT EXISTS (
                SELECT 1 FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id
                WHERE cp.project_id = p.id AND (lower(c.name) = n.value OR (c.system_key = 'favourites' AND n.value IN ('favourites', 'favorites')))))
        AND NOT EXISTS (
            SELECT 1 FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id, json_each(?{j}, '$.not_collections') n
            WHERE cp.project_id = p.id AND (lower(c.name) = n.value OR (c.system_key = 'favourites' AND n.value IN ('favourites', 'favorites'))))
        AND (json_extract(?{j}, '$.favourite') IS NULL OR json_extract(?{j}, '$.favourite') = EXISTS (
            SELECT 1 FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id
            WHERE cp.project_id = p.id AND c.system_key = 'favourites'))
        AND (json_array_length(?{j}, '$.sources') = 0 OR EXISTS (
            SELECT 1 FROM json_each(?{j}, '$.sources') s WHERE instr(lower(COALESCE(p.source_url, '')), s.value) > 0))
        AND NOT EXISTS (
            SELECT 1 FROM json_each(?{j}, '$.authors') n WHERE instr(lower(COALESCE(p.source_author, '')), n.value) = 0)
        AND NOT EXISTS (
            SELECT 1 FROM json_each(?{j}, '$.files') n WHERE NOT EXISTS (
                SELECT 1 FROM assets a WHERE a.project_id = p.id AND instr(lower(a.file_path), n.value) > 0))
        "#
    )
}

/// Lists projects in `sort` order. `after` is the previous page's last `(sort value, id)`.
/// Returns each row with its sort value, for the next cursor.
pub async fn list_projects(
    pool: &Pool<Sqlite>,
    limit: i64,
    after: Option<(serde_json::Value, String)>,
    sort: ProjectSort,
    descending: bool,
    filter: &ProjectFilter,
) -> Result<Vec<(ProjectRow, serde_json::Value)>, sqlx::Error> {
    let filter_json = filter.bind_json();
    // Only fixed strings go into the SQL; values are bound.
    let (cmp, dir) = if descending { ("<", "DESC") } else { (">", "ASC") };
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
        expr = sort.sql_expr(),
        filter = filter_sql(4),
    );

    let numeric = sort.is_numeric();
    let query = sqlx::query(&sql);
    let query = match &after {
        None => query.bind(None::<String>).bind(None::<String>),
        Some((value, id)) if numeric => query.bind(value.as_i64().ok_or_else(invalid_cursor)?).bind(id),
        Some((value, id)) => query.bind(value.as_str().ok_or_else(invalid_cursor)?).bind(id),
    };
    let rows = query.bind(limit).bind(filter_json).fetch_all(pool).await?;

    rows.iter()
        .map(|row| {
            let value = if numeric {
                serde_json::Value::from(row.try_get::<i64, _>("sort_value")?)
            } else {
                serde_json::Value::String(row.try_get::<String, _>("sort_value")?)
            };
            Ok((ProjectRow::from_row(row)?, value))
        })
        .collect()
}

/// The cursor's sort value doesn't fit the sort (e.g. text for a size sort).
pub const INVALID_CURSOR: &str = "invalid cursor value";

fn invalid_cursor() -> sqlx::Error {
    sqlx::Error::Protocol(INVALID_CURSOR.into())
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
        let all = |f: ProjectFilter| async move { names(list_projects(pool, 50, None, ProjectSort::Updated, true, &f).await.unwrap().into_iter().map(|(p, _)| p).collect()) };

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
        let after = page1.last().map(|(p, v)| (v.clone(), p.id.clone()));
        assert_eq!(names(page1.into_iter().map(|p| p.0).collect()), vec!["dragon", "bins"]);
        let page2 = list_projects(pool, 2, after, ProjectSort::Updated, true, &f).await.unwrap();
        assert_eq!(names(page2.into_iter().map(|p| p.0).collect()), vec!["duct"]);

        // Search combines with filters.
        let hits = crate::queries::projects_search::search_projects(pool, "d", 50, None, &ProjectFilter { formats: vec!["STEP".into()], ..Default::default() })
            .await
            .unwrap();
        assert_eq!(hits.into_iter().map(|h| h.project.name).collect::<Vec<_>>(), vec!["duct"]);
        let _ = std::fs::remove_dir_all(dir);
    }
    
    #[tokio::test]
    async fn search_language_filters() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        let dragon = project(pool, "dragon", "2026-01-05T00:00:00Z", &[("parts/Dragon_Head.stl", "model"), ("p.jpg", "image")], &["Silk PLA", "gift"]).await;
        let bins = project(pool, "bins", "2026-01-04T00:00:00Z", &[("bins.3mf", "model")], &["Silk PLA"]).await;
        project(pool, "duct", "2026-01-03T00:00:00Z", &[("duct.step", "model")], &[]).await;
        sqlx::query("UPDATE projects SET source_url = 'https://www.printables.com/model/1-dragon', source_author = 'Prusa Research' WHERE id = ?1")
            .bind(&dragon).execute(pool).await.unwrap();
        let desk = crate::queries::collections::create_collection(pool, "Living Room", "", T).await.unwrap();
        crate::queries::collections::add_project(pool, &desk.id, &bins, T).await.unwrap();
        let fav = crate::queries::collections::ensure_favourites(pool, T).await.unwrap();
        crate::queries::collections::add_project(pool, &fav, &dragon, T).await.unwrap();

        let q = |query: &'static str| async move {
            let mut f = ProjectFilter::default();
            let text = crate::queries::search_syntax::parse_into(query, &mut f).text;
            assert!(text.is_empty(), "{query} left words: {text}");
            names(list_projects(pool, 50, None, ProjectSort::Updated, true, &f).await.unwrap().into_iter().map(|(p, _)| p).collect())
        };
        assert_eq!(q(r##"#"silk pla""##).await, vec!["dragon", "bins"]);
        assert_eq!(q(r##"#"silk pla" -#gift"##).await, vec!["bins"]);
        assert_eq!(q(r#"in:"living room""#).await, vec!["bins"]);
        assert_eq!(q(r#"-in:"living room""#).await, vec!["dragon", "duct"]);
        assert_eq!(q("in:favourites").await, vec!["dragon"]);
        assert_eq!(q("is:fav").await, vec!["dragon"]);
        assert_eq!(q("-is:fav").await, vec!["bins", "duct"]);
        assert_eq!(q("has:image").await, vec!["dragon"]);
        assert_eq!(q("no:image").await, vec!["bins", "duct"]);
        assert_eq!(q("from:printables").await, vec!["dragon"]);
        assert_eq!(q("by:prusa").await, vec!["dragon"]);
        assert_eq!(q("file:head").await, vec!["dragon"], "file paths, ignoring case");
        assert_eq!(q("format:step").await, vec!["duct"]);
        assert_eq!(q("#nope").await, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Inserts a project with its own created/updated dates and one asset per size.
    async fn sized(pool: &Pool<Sqlite>, name: &str, day: u32, sizes: &[i64]) {
        let id = Uuid::new_v4().to_string();
        let created = format!("2026-01-{day:02}T00:00:00Z");
        // Updated order is the reverse of created order.
        let updated = format!("2026-02-{:02}T00:00:00Z", 10 - day);
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?4)")
            .bind(&id).bind(name).bind(&created).bind(&updated).execute(pool).await.unwrap();
        for (i, size) in sizes.iter().enumerate() {
            sqlx::query("INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, created_at, updated_at) VALUES (?1, ?2, ?3, 'model', ?4, ?5, ?5, ?5)")
                .bind(Uuid::new_v4().to_string()).bind(&id).bind(format!("f{i}.stl")).bind(size).bind(T).execute(pool).await.unwrap();
        }
    }

    /// Pages through the whole list `page` rows at a time, like the infinite scroll does.
    async fn collect_all(pool: &Pool<Sqlite>, sort: ProjectSort, desc: bool, page: i64) -> Vec<String> {
        let (mut out, mut after) = (Vec::new(), None);
        loop {
            let rows = list_projects(pool, page, after.clone(), sort, desc, &ProjectFilter::default()).await.unwrap();
            let full = rows.len() as i64 == page;
            if let Some((last, value)) = rows.last() {
                after = Some((value.clone(), last.id.clone()));
            }
            out.extend(rows.into_iter().map(|(p, _)| p.name));
            if !full {
                return out;
            }
        }
    }

    #[tokio::test]
    async fn sorts_and_paginates() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        // name, created day, asset sizes (charlie and Delta tie on total size).
        sized(pool, "Delta", 1, &[300]).await;
        sized(pool, "alpha", 2, &[10, 20, 30, 40]).await;
        sized(pool, "echo", 3, &[]).await;
        sized(pool, "charlie", 4, &[100, 200]).await;
        sized(pool, "Bravo", 5, &[5000]).await;

        // File dates follow the created day; echo has no files.
        sqlx::query("UPDATE assets SET mtime = (SELECT created_at FROM projects WHERE projects.id = assets.project_id)")
            .execute(pool).await.unwrap();

        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        use ProjectSort::*;
        assert_eq!(collect_all(pool, Name, false, 50).await, s(&["alpha", "Bravo", "charlie", "Delta", "echo"]), "case-insensitive");
        assert_eq!(collect_all(pool, Name, true, 50).await, s(&["echo", "Delta", "charlie", "Bravo", "alpha"]));
        assert_eq!(collect_all(pool, Created, false, 50).await, s(&["Delta", "alpha", "echo", "charlie", "Bravo"]));
        assert_eq!(collect_all(pool, Modified, true, 50).await, s(&["Bravo", "charlie", "alpha", "Delta", "echo"]), "no files sorts as oldest");
        assert_eq!(collect_all(pool, Updated, true, 50).await, s(&["Delta", "alpha", "echo", "charlie", "Bravo"]));
        // Files: alpha 4, charlie 2, Delta 1, Bravo 1, echo 0.
        let by_files = collect_all(pool, Files, true, 50).await;
        assert_eq!(by_files[..2], s(&["alpha", "charlie"]));
        assert_eq!(by_files[4], "echo");
        let by_size = collect_all(pool, Size, false, 50).await;
        assert_eq!(by_size[..2], s(&["echo", "alpha"]));
        assert_eq!(by_size[4], "Bravo");
        // Ties are broken by id, which is random here: either order, but stable across pages.
        let mut tie = by_size[2..4].to_vec();
        tie.sort();
        assert_eq!(tie, s(&["Delta", "charlie"]));

        // Small pages return exactly the same sequence: no gaps or repeats at page edges.
        for sort in [Updated, Created, Modified, Name, Size, Files] {
            for desc in [false, true] {
                let whole = collect_all(pool, sort, desc, 50).await;
                assert_eq!(whole.len(), 5);
                for page in [1, 2, 3] {
                    assert_eq!(collect_all(pool, sort, desc, page).await, whole, "{sort:?} desc={desc} page={page}");
                }
            }
        }

        // A cursor value of the wrong type is rejected, not misread.
        let err = list_projects(pool, 2, Some((serde_json::Value::from("x"), "id".into())), Size, true, &ProjectFilter::default()).await;
        assert!(matches!(err, Err(sqlx::Error::Protocol(_))));
        let _ = std::fs::remove_dir_all(dir);
    }
}
