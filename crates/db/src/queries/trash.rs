//! Deleted projects kept for a while so they can be restored. The folder moves are done by
//! the caller; these queries only keep the records.

use sqlx::{FromRow, Pool, Sqlite};

#[derive(Debug, Clone, FromRow)]
pub struct TrashRow {
    pub id: String,
    pub folder_path: String,
    pub name: String,
    pub file_count: i64,
    pub size_bytes: i64,
    pub snapshot: String,
    pub trashed_at: String,
    /// Set when one file was trashed rather than a whole project (relative to `folder_path`).
    pub file_path: Option<String>,
}

/// What's needed to move a project to the trash: its folder and a few facts for the list.
#[derive(Debug, Clone, FromRow)]
pub struct TrashCandidate {
    pub folder_path: String,
    pub name: String,
    pub file_count: i64,
    pub size_bytes: i64,
}

pub async fn candidate(pool: &Pool<Sqlite>, project_id: &str) -> Result<Option<TrashCandidate>, sqlx::Error> {
    sqlx::query_as::<_, TrashCandidate>(
        r#"
        SELECT p.folder_path, p.name,
               (SELECT COUNT(*) FROM assets a WHERE a.project_id = p.id) AS file_count,
               (SELECT COALESCE(SUM(size_bytes), 0) FROM assets a WHERE a.project_id = p.id) AS size_bytes
        FROM projects p WHERE p.id = ?1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

/// Records the trashed project and removes it from the index, in one transaction.
pub async fn trash_project(pool: &Pool<Sqlite>, project_id: &str, row: &TrashRow) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO trash (id, folder_path, name, file_count, size_bytes, snapshot, trashed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&row.id)
    .bind(&row.folder_path)
    .bind(&row.name)
    .bind(row.file_count)
    .bind(row.size_bytes)
    .bind(&row.snapshot)
    .bind(&row.trashed_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM projects WHERE id = ?1").bind(project_id).execute(&mut *tx).await?;
    tx.commit().await
}

/// What's needed to trash one file.
#[derive(Debug, Clone, FromRow)]
pub struct FileCandidate {
    pub folder_path: String,
    pub project_name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub is_cover: bool,
}

pub async fn file_candidate(pool: &Pool<Sqlite>, project_id: &str, asset_id: &str) -> Result<Option<FileCandidate>, sqlx::Error> {
    sqlx::query_as::<_, FileCandidate>(
        "SELECT p.folder_path, p.name AS project_name, a.file_path, a.size_bytes, (p.main_image_id = a.id) AS is_cover
         FROM assets a JOIN projects p ON p.id = a.project_id WHERE a.id = ?1 AND a.project_id = ?2",
    )
    .bind(asset_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

/// Records a trashed file and removes its asset (and cover, if it was one), in one transaction.
pub async fn trash_file(pool: &Pool<Sqlite>, project_id: &str, asset_id: &str, row: &TrashRow) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO trash (id, folder_path, name, file_count, size_bytes, snapshot, trashed_at, file_path) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7)",
    )
    .bind(&row.id)
    .bind(&row.folder_path)
    .bind(&row.name)
    .bind(row.size_bytes)
    .bind(&row.snapshot)
    .bind(&row.trashed_at)
    .bind(&row.file_path)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE projects SET main_image_id = NULL WHERE id = ?1 AND main_image_id = ?2").bind(project_id).bind(asset_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM assets WHERE id = ?1 AND project_id = ?2").bind(asset_id).bind(project_id).execute(&mut *tx).await?;
    tx.commit().await
}

/// Newest first.
pub async fn list(pool: &Pool<Sqlite>) -> Result<Vec<TrashRow>, sqlx::Error> {
    sqlx::query_as::<_, TrashRow>("SELECT * FROM trash ORDER BY trashed_at DESC, id").fetch_all(pool).await
}

pub async fn get(pool: &Pool<Sqlite>, id: &str) -> Result<Option<TrashRow>, sqlx::Error> {
    sqlx::query_as::<_, TrashRow>("SELECT * FROM trash WHERE id = ?1").bind(id).fetch_optional(pool).await
}

/// Items trashed before `cutoff` (RFC 3339).
pub async fn older_than(pool: &Pool<Sqlite>, cutoff: &str) -> Result<Vec<TrashRow>, sqlx::Error> {
    sqlx::query_as::<_, TrashRow>("SELECT * FROM trash WHERE trashed_at < ?1").bind(cutoff).fetch_all(pool).await
}

pub async fn remove(pool: &Pool<Sqlite>, id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM trash WHERE id = ?1").bind(id).execute(pool).await?.rows_affected() > 0)
}
