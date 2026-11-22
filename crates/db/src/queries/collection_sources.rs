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

#[derive(Debug, Clone, FromRow)]
pub struct OverviewRow {
    pub collection_id: String,
    pub name: String,
    pub source: String,
    pub url: String,
    pub keep_in_sync: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

/// Every imported collection with its name.
pub async fn overview(pool: &Pool<Sqlite>) -> Result<Vec<OverviewRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.collection_id, c.name, s.source, s.url, s.keep_in_sync, s.last_synced_at, s.last_error
         FROM collection_sources s JOIN collections c ON c.id = s.collection_id",
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct RunRow {
    pub id: String,
    pub collection_id: String,
    pub collection_name: String,
    pub source: String,
    pub reason: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub imported: i64,
    pub linked: i64,
    pub removed: i64,
    pub failures_json: String,
    pub error: Option<String>,
}

const RUN_COLUMNS: &str =
    "id, collection_id, collection_name, source, reason, status, started_at, finished_at, imported, linked, removed, failures_json, error";

pub async fn create_run(pool: &Pool<Sqlite>, id: &str, collection_id: &str, collection_name: &str, source: &str, reason: &str, now: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO collection_sync_runs (id, collection_id, collection_name, source, reason, status, started_at) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
    )
    .bind(id)
    .bind(collection_id)
    .bind(collection_name)
    .bind(source)
    .bind(reason)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_run(
    pool: &Pool<Sqlite>,
    id: &str,
    status: &str,
    imported: i64,
    linked: i64,
    removed: i64,
    failures_json: &str,
    error: Option<&str>,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE collection_sync_runs SET status = ?2, imported = ?3, linked = ?4, removed = ?5, failures_json = ?6, error = ?7, finished_at = ?8 WHERE id = ?1",
    )
    .bind(id)
    .bind(status)
    .bind(imported)
    .bind(linked)
    .bind(removed)
    .bind(failures_json)
    .bind(error)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Runs left `running` by a restart can't finish any more.
pub async fn fail_interrupted_runs(pool: &Pool<Sqlite>, now: &str) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("UPDATE collection_sync_runs SET status = 'failed', error = 'Interrupted by a server restart', finished_at = ?1 WHERE status = 'running'")
        .bind(now)
        .execute(pool)
        .await?
        .rows_affected())
}

pub async fn get_run(pool: &Pool<Sqlite>, id: &str) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {RUN_COLUMNS} FROM collection_sync_runs WHERE id = ?1")).bind(id).fetch_optional(pool).await
}

/// Newest first, before `(started_at, id)` when given.
pub async fn list_runs(pool: &Pool<Sqlite>, limit: i64, before: Option<(&str, &str)>) -> Result<Vec<RunRow>, sqlx::Error> {
    match before {
        None => {
            sqlx::query_as(&format!("SELECT {RUN_COLUMNS} FROM collection_sync_runs ORDER BY started_at DESC, id DESC LIMIT ?1"))
                .bind(limit)
                .fetch_all(pool)
                .await
        }
        Some((started, id)) => {
            sqlx::query_as(&format!(
                "SELECT {RUN_COLUMNS} FROM collection_sync_runs WHERE (started_at, id) < (?1, ?2) ORDER BY started_at DESC, id DESC LIMIT ?3"
            ))
            .bind(started)
            .bind(id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}
