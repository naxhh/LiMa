use lima_domain::models::sync::{SyncEventRow, SyncRunRow, SyncStats};
use lima_domain::pagination::Cursor;
use sqlx::{Pool, Sqlite};

const RUN_COLUMNS: &str =
    "id, mode, reason, status, stats_json, started_at, finished_at, error";

pub async fn create_run(
    pool: &Pool<Sqlite>,
    id: &str,
    mode: &str,
    reason: &str,
    now: &str,
) -> Result<SyncRunRow, sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO sync_runs (id, mode, reason, status, options_json, stats_json, started_at, finished_at, error)
           VALUES (?1, ?2, ?3, 'running', '{}', ?4, ?5, NULL, '')"#,
    )
    .bind(id)
    .bind(mode)
    .bind(reason)
    .bind(serde_json::to_string(&SyncStats::default()).unwrap())
    .bind(now)
    .execute(pool)
    .await?;

    get_run(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn update_run_stats(
    pool: &Pool<Sqlite>,
    id: &str,
    stats: &SyncStats,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE sync_runs SET stats_json = ?1 WHERE id = ?2"#)
        .bind(serde_json::to_string(stats).unwrap())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_run(
    pool: &Pool<Sqlite>,
    id: &str,
    status: &str,
    stats: &SyncStats,
    error: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE sync_runs SET status = ?1, stats_json = ?2, error = ?3, finished_at = ?4 WHERE id = ?5"#,
    )
    .bind(status)
    .bind(serde_json::to_string(stats).unwrap())
    .bind(error)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Runs left as queued/running belong to a previous process that died; mark them failed.
pub async fn fail_interrupted_runs(pool: &Pool<Sqlite>, now: &str) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        r#"UPDATE sync_runs
           SET status = 'failed', error = 'Interrupted: server stopped while the run was in progress', finished_at = ?1
           WHERE status IN ('queued', 'running')"#,
    )
    .bind(now)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn get_run(pool: &Pool<Sqlite>, id: &str) -> Result<Option<SyncRunRow>, sqlx::Error> {
    sqlx::query_as::<_, SyncRunRow>(&format!("SELECT {RUN_COLUMNS} FROM sync_runs WHERE id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn latest_run(pool: &Pool<Sqlite>) -> Result<Option<SyncRunRow>, sqlx::Error> {
    sqlx::query_as::<_, SyncRunRow>(&format!(
        "SELECT {RUN_COLUMNS} FROM sync_runs ORDER BY started_at DESC, id DESC LIMIT 1"
    ))
    .fetch_optional(pool)
    .await
}

/// Newest first. The cursor reuses `updated_at` to carry `started_at`.
pub async fn list_runs(
    pool: &Pool<Sqlite>,
    limit: i64,
    cursor: Option<Cursor>,
) -> Result<Vec<SyncRunRow>, sqlx::Error> {
    match cursor {
        None => {
            sqlx::query_as::<_, SyncRunRow>(&format!(
                "SELECT {RUN_COLUMNS} FROM sync_runs ORDER BY started_at DESC, id DESC LIMIT ?1"
            ))
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        Some(c) => {
            sqlx::query_as::<_, SyncRunRow>(&format!(
                "SELECT {RUN_COLUMNS} FROM sync_runs
                 WHERE (started_at, id) < (?1, ?2)
                 ORDER BY started_at DESC, id DESC LIMIT ?3"
            ))
            .bind(c.updated_at)
            .bind(c.id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

pub async fn insert_event(
    pool: &Pool<Sqlite>,
    run_id: &str,
    level: &str,
    kind: &str,
    data: &serde_json::Value,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO sync_events (run_id, created_at, level, kind, data_json) VALUES (?1, ?2, ?3, ?4, ?5)"#,
    )
    .bind(run_id)
    .bind(now)
    .bind(level)
    .bind(kind)
    .bind(data.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Oldest first, starting after event id `after` (exclusive). Suited for tailing a live run.
pub async fn list_events(
    pool: &Pool<Sqlite>,
    run_id: &str,
    after: i64,
    limit: i64,
) -> Result<Vec<SyncEventRow>, sqlx::Error> {
    sqlx::query_as::<_, SyncEventRow>(
        r#"SELECT id, run_id, created_at, level, kind, data_json
           FROM sync_events
           WHERE run_id = ?1 AND id > ?2
           ORDER BY id ASC
           LIMIT ?3"#,
    )
    .bind(run_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
}
