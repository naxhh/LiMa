use sqlx::{Sqlite, Pool};
use lima_domain::models::project::ProjectRow;

pub async fn list_projects(
    pool: &Pool<Sqlite>,
    limit: i64,
    cursor: Option<(String, String)>, // update_at, id
) -> Result<Vec<ProjectRow>, sqlx::Error> {
    match cursor {
        None => { list_projects_from_start(pool, limit).await },
        Some((updated_at, id)) => { list_projects_from_cursor(pool, limit, &updated_at, &id).await },
    }
}

async fn list_projects_from_start(
    pool: &Pool<Sqlite>,
    limit: i64
) -> Result<Vec<ProjectRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectRow>(
        r#"
        SELECT id, folder_path, name, description, main_image_id, created_at, updated_at, last_scanned_at
        FROM projects
        ORDER BY updated_at DESC, id DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

async fn list_projects_from_cursor(
    pool: &Pool<Sqlite>,
    limit: i64,
    updated_at: &str,
    id: &str,
) -> Result<Vec<ProjectRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectRow>(
        r#"
        SELECT id, folder_path, name, description, main_image_id, created_at, updated_at, last_scanned_at
        FROM projects
        WHERE (updated_at, id) < (?1, ?2)
        ORDER BY updated_at DESC, id DESC
        LIMIT ?3
        "#,
    )
    .bind(updated_at)
    .bind(id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
