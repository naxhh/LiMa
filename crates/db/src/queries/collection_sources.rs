//! Collections imported from a site (MakerWorld, Thingiverse), and what sync knows about them.

use sqlx::{FromRow, Pool, Sqlite};

#[derive(Debug, Clone, FromRow)]
pub struct SourceRow {
    pub id: String,
    pub collection_id: String,
    pub source: String,
    pub source_id: String,
    pub url: String,
    pub keep_in_sync: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct ItemRow {
    pub model_id: String,
    pub model_url: String,
    pub name: String,
    pub project_id: Option<String>,
    pub imported: bool,
    pub excluded: bool,
    pub last_error: Option<String>,
}

const COLUMNS: &str = "id, collection_id, source, source_id, url, keep_in_sync, last_synced_at, last_error, created_at";

pub async fn create(pool: &Pool<Sqlite>, row: &SourceRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO collection_sources (id, collection_id, source, source_id, url, keep_in_sync, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&row.id)
    .bind(&row.collection_id)
    .bind(&row.source)
    .bind(&row.source_id)
    .bind(&row.url)
    .bind(row.keep_in_sync)
    .bind(&row.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &Pool<Sqlite>, id: &str) -> Result<Option<SourceRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM collection_sources WHERE id = ?1")).bind(id).fetch_optional(pool).await
}

pub async fn by_collection(pool: &Pool<Sqlite>, collection_id: &str) -> Result<Option<SourceRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM collection_sources WHERE collection_id = ?1")).bind(collection_id).fetch_optional(pool).await
}

pub async fn by_source(pool: &Pool<Sqlite>, source: &str, source_id: &str) -> Result<Option<SourceRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM collection_sources WHERE source = ?1 AND source_id = ?2"))
        .bind(source)
        .bind(source_id)
        .fetch_optional(pool)
        .await
}

pub async fn synced(pool: &Pool<Sqlite>) -> Result<Vec<SourceRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM collection_sources WHERE keep_in_sync = 1 ORDER BY created_at")).fetch_all(pool).await
}

pub async fn set_keep_in_sync(pool: &Pool<Sqlite>, id: &str, keep: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE collection_sources SET keep_in_sync = ?2 WHERE id = ?1").bind(id).bind(keep).execute(pool).await?;
    Ok(())
}

pub async fn set_result(pool: &Pool<Sqlite>, id: &str, synced_at: &str, error: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE collection_sources SET last_synced_at = ?2, last_error = ?3 WHERE id = ?1")
        .bind(id)
        .bind(synced_at)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn items(pool: &Pool<Sqlite>, source_ref: &str) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT model_id, model_url, name, project_id, imported, excluded, last_error FROM collection_source_items WHERE source_ref = ?1",
    )
    .bind(source_ref)
    .fetch_all(pool)
    .await
}

pub async fn upsert_item(pool: &Pool<Sqlite>, source_ref: &str, item: &ItemRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO collection_source_items (source_ref, model_id, model_url, name, project_id, imported, excluded, last_error)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(source_ref, model_id) DO UPDATE SET
          model_url = excluded.model_url, name = excluded.name, project_id = excluded.project_id,
          imported = excluded.imported, excluded = excluded.excluded, last_error = excluded.last_error
        "#,
    )
    .bind(source_ref)
    .bind(&item.model_id)
    .bind(&item.model_url)
    .bind(&item.name)
    .bind(&item.project_id)
    .bind(item.imported)
    .bind(item.excluded)
    .bind(&item.last_error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_item(pool: &Pool<Sqlite>, source_ref: &str, model_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM collection_source_items WHERE source_ref = ?1 AND model_id = ?2").bind(source_ref).bind(model_id).execute(pool).await?;
    Ok(())
}

/// The project imported from this model page, if any. URLs are compared without a trailing
/// slash or query, and model pages are matched by their id-bearing prefix (slugs change).
pub async fn project_for_url(pool: &Pool<Sqlite>, url: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM projects WHERE source_url = ?1 OR rtrim(source_url, '/') = rtrim(?1, '/') LIMIT 1")
        .bind(url)
        .fetch_optional(pool)
        .await
}

pub async fn set_token(pool: &Pool<Sqlite>, name: &str, token: &str, now: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO integration_tokens (name, token, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(name) DO UPDATE SET token = excluded.token, updated_at = excluded.updated_at")
        .bind(name)
        .bind(token)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn token(pool: &Pool<Sqlite>, name: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT token FROM integration_tokens WHERE name = ?1").bind(name).fetch_optional(pool).await
}
