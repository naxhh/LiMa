//! Queries used by the indexer to reconcile the database with the library folder.

use sqlx::{FromRow, Pool, Sqlite, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct IndexedProject {
    pub id: String,
    pub folder_path: String,
    pub main_image_id: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct IndexedAsset {
    pub id: String,
    pub file_path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub mtime: String,
}

pub async fn list_indexed_projects(pool: &Pool<Sqlite>) -> Result<Vec<IndexedProject>, sqlx::Error> {
    sqlx::query_as::<_, IndexedProject>(
        r#"SELECT id, folder_path, main_image_id FROM projects"#,
    )
    .fetch_all(pool)
    .await
}

pub async fn list_indexed_assets(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
) -> Result<Vec<IndexedAsset>, sqlx::Error> {
    sqlx::query_as::<_, IndexedAsset>(
        r#"SELECT id, file_path, kind, size_bytes, mtime FROM assets WHERE project_id = ?1"#,
    )
    .bind(project_id)
    .fetch_all(&mut **tx)
    .await
}

/// Inserts a project discovered on disk. Name defaults to the folder name.
pub async fn insert_discovered_project(
    tx: &mut Transaction<'_, Sqlite>,
    folder_path: &str,
    now: &str,
) -> Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO projects (id, folder_path, name, description, main_image_id, created_at, updated_at, last_scanned_at)
           VALUES (?1, ?2, ?2, '', NULL, ?3, ?3, ?3)"#,
    )
    .bind(&id)
    .bind(folder_path)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// Removes the DB row of a project whose folder no longer exists. Assets/tags cascade.
pub async fn remove_missing_project(pool: &Pool<Sqlite>, project_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM projects WHERE id = ?1"#)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_discovered_asset(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    file_path: &str,
    kind: &str,
    size_bytes: i64,
    mtime: &str,
    mime: &str,
    now: &str,
) -> Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, mime, file_hash, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8)"#,
    )
    .bind(&id)
    .bind(project_id)
    .bind(file_path)
    .bind(kind)
    .bind(size_bytes)
    .bind(mtime)
    .bind(mime)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// File content changed on disk: refresh its stat data and drop the stale hash.
pub async fn update_changed_asset(
    tx: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
    size_bytes: i64,
    mtime: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE assets SET size_bytes = ?1, mtime = ?2, file_hash = NULL, updated_at = ?3 WHERE id = ?4"#,
    )
    .bind(size_bytes)
    .bind(mtime)
    .bind(now)
    .bind(asset_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn remove_missing_asset(
    tx: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM assets WHERE id = ?1"#)
        .bind(asset_id)
        .execute(&mut **tx)
        .await?;
    // main_image_id has no FK constraint, so clear dangling references manually.
    sqlx::query(r#"UPDATE projects SET main_image_id = NULL WHERE main_image_id = ?1"#)
        .bind(asset_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn set_main_image(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    asset_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE projects SET main_image_id = ?1 WHERE id = ?2"#)
        .bind(asset_id)
        .bind(project_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Marks a project as scanned. When `changed` also bumps `updated_at` so it surfaces in listings.
pub async fn mark_project_scanned(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    changed: bool,
    now: &str,
) -> Result<(), sqlx::Error> {
    let sql = if changed {
        r#"UPDATE projects SET last_scanned_at = ?1, updated_at = ?1 WHERE id = ?2"#
    } else {
        r#"UPDATE projects SET last_scanned_at = ?1 WHERE id = ?2"#
    };
    sqlx::query(sql).bind(now).bind(project_id).execute(&mut **tx).await?;
    Ok(())
}
