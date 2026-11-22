
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::Cursor;
use crate::queries::projects::{ProjectFilter, filter_sql};
use sqlx::{Pool, Sqlite, FromRow};

#[derive(Debug)]
pub struct SearchProjectRow {
    pub rank: f64,
    pub project: ProjectRow
}

#[derive(FromRow)]
struct SearchRow {
    rank: f64,
    id: String,
    folder_path: String,
    name: String,
    description: String,
    main_image_id: Option<String>,
    created_at: String,
    updated_at: String,
    last_scanned_at: Option<String>,
}

impl From<SearchRow> for SearchProjectRow {
    fn from(row: SearchRow) -> Self {
        Self {
            rank: row.rank,
            project: ProjectRow {
                id: row.id,
                folder_path: row.folder_path,
                name: row.name,
                description: row.description,
                main_image_id: row.main_image_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                last_scanned_at: row.last_scanned_at,
            }
        }
    }
}

/// Turns free text into an FTS5 query: every word becomes a quoted prefix term
/// (`"drag"*`) and all terms must match. Punctuation only separates words, so
/// input like `print-in-place` or `a "b` can't produce an FTS syntax error.
/// Returns `None` when the text has no searchable characters.
pub fn to_fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if terms.is_empty() { None } else { Some(terms.join(" ")) }
}

pub async fn search_projects(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64,
    cursor: Option<Cursor>,
    filter: &ProjectFilter,
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    let Some(fts) = to_fts_query(query) else { return Ok(Vec::new()) };
    let query = fts.as_str();
    match cursor {
        None => { search_projects_from_start(pool, query, limit, filter).await }
        Some(cursor) => { search_projects_from_cursor(pool, query, limit, &cursor, filter).await }
    }
}

async fn search_projects_from_start(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64,
    filter: &ProjectFilter,
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    let filter_json = filter.bind_json();
    let sql = format!(
        r#"
        SELECT
            COALESCE(bm25(projects_fts), 0.0) AS rank,
            p.id,
            p.folder_path,
            p.name,
            p.description,
            p.main_image_id,
            p.created_at,
            p.updated_at,
            p.last_scanned_at
        FROM projects_fts
        JOIN projects p ON projects_fts.project_id = p.id
        WHERE projects_fts MATCH ?1
        {filter}
        ORDER BY rank ASC, p.updated_at DESC, p.id DESC
        LIMIT ?2
        "#,
        filter = filter_sql(3)
    );
    let rows = sqlx::query_as::<_, SearchRow>(&sql)
    .bind(query)
    .bind(limit)
    .bind(&filter_json)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

async fn search_projects_from_cursor(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64,
    cursor: &Cursor,
    filter: &ProjectFilter,
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    let filter_json = filter.bind_json();
    let sql = format!(
        r#"
        WITH ranked AS(
            SELECT
                COALESCE(bm25(projects_fts), 0.0) AS rank,
                p.id,
                p.folder_path,
                p.name,
                p.description,
                p.main_image_id,
                p.created_at,
                p.updated_at,
                p.last_scanned_at
            FROM projects_fts
            JOIN projects p ON projects_fts.project_id = p.id
            WHERE projects_fts MATCH ?1
            {filter}
        )
        SELECT *
        FROM ranked
        WHERE (rank > ?2)
              OR (rank = ?2 AND (updated_at < ?3)
              OR (rank = ?2 AND updated_at = ?3 AND id < ?4))
        ORDER BY rank ASC, updated_at DESC, id DESC
        LIMIT ?5
        "#,
        filter = filter_sql(6)
    );
    let rows = sqlx::query_as::<_, SearchRow>(&sql)
     .bind(query)
     .bind(cursor.rank)
     .bind(cursor.updated_at.clone())
    .bind(cursor.id.clone())
    .bind(limit)
    .bind(&filter_json)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::to_fts_query;

    #[test]
    fn fts_query_quotes_words_as_prefixes() {
        assert_eq!(to_fts_query("print-in-place").as_deref(), Some(r#""print"* "in"* "place"*"#));
        assert_eq!(to_fts_query("  Drag  ").as_deref(), Some(r#""Drag"*"#));
        assert_eq!(to_fts_query(r#"a "b OR c*"#).as_deref(), Some(r#""a"* "b"* "OR"* "c"*"#));
        assert_eq!(to_fts_query("ñandú 3mf").as_deref(), Some(r#""ñandú"* "3mf"*"#));
        assert_eq!(to_fts_query(" -\"* ").as_deref(), None);
    }
}
