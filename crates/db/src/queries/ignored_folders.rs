//! Folders removed from LiMa without deleting their files. Sync leaves them alone.

use std::collections::HashSet;

use sqlx::{Pool, Sqlite};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IgnoredFolder {
    pub folder_path: String,
    pub created_at: String,
}

pub async fn list_ignored(pool: &Pool<Sqlite>) -> Result<Vec<IgnoredFolder>, sqlx::Error> {
    sqlx::query_as::<_, IgnoredFolder>("SELECT folder_path, created_at FROM ignored_folders ORDER BY folder_path")
        .fetch_all(pool)
        .await
}

pub async fn ignored_set(pool: &Pool<Sqlite>) -> Result<HashSet<String>, sqlx::Error> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT folder_path FROM ignored_folders").fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

/// Lets sync index the folder again. Returns false if it wasn't ignored.
pub async fn unignore(pool: &Pool<Sqlite>, folder_path: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM ignored_folders WHERE folder_path = ?1").bind(folder_path).execute(pool).await?;
    Ok(done.rows_affected() > 0)
}
