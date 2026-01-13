
use lima_domain::models::project::ProjectRow;
use lima_domain::pagination::Cursor;
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

pub async fn search_projects(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64,
    cursor: Option<Cursor>,
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    match cursor {
        None => { search_projects_from_start(pool, query, limit).await }
        Some(cursor) => { search_projects_from_cursor(pool, query, limit, &cursor).await }
    }
}

async fn search_projects_from_start(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SearchRow>(
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
        ORDER BY rank ASC, p.updated_at DESC, p.id DESC
        LIMIT ?2
        "#,
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

async fn search_projects_from_cursor(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i64,
    cursor: &Cursor
) -> Result<Vec<SearchProjectRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SearchRow>(
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
        )
        SELECT *
        FROM ranked
        WHERE (rank > ?2)
              OR (rank = ?2 AND (updated_at < ?3)
              OR (rank = ?2 AND updated_at = ?3 AND id < ?4))
        ORDER BY rank ASC, updated_at DESC, id DESC
        LIMIT ?5
        "#,
    )
     .bind(query)
     .bind(cursor.rank)
     .bind(cursor.updated_at.clone())
    .bind(cursor.id.clone())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}
