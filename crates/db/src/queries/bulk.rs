//! Changes applied to many projects at once (one statement per action, ids as a JSON array).
//! Ids that don't exist are skipped. Each function returns how many links it added or removed.

use sqlx::{Pool, Sqlite};

fn ids_json(ids: &[String]) -> String {
    serde_json::to_string(ids).expect("serialize ids")
}

/// Adds tags (created if needed, matched ignoring case) to every project.
pub async fn add_tags(pool: &Pool<Sqlite>, project_ids: &[String], names: &[String], now: &str) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let tag_ids = crate::queries::tags::ensure_tags(&mut tx, names, now).await?;
    let res = sqlx::query(
        r#"
        INSERT OR IGNORE INTO project_tags (project_id, tag_id)
        SELECT p.id, t.value FROM projects p, json_each(?2) t
        WHERE p.id IN (SELECT value FROM json_each(?1))
        "#,
    )
    .bind(ids_json(project_ids))
    .bind(ids_json(&tag_ids))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected())
}

/// Removes tags (by name, ignoring case) from every project. The tags themselves stay.
pub async fn remove_tags(pool: &Pool<Sqlite>, project_ids: &[String], names: &[String]) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        r#"
        DELETE FROM project_tags
        WHERE project_id IN (SELECT value FROM json_each(?1))
          AND tag_id IN (SELECT id FROM tags WHERE lower(name) IN (SELECT lower(value) FROM json_each(?2)))
        "#,
    )
    .bind(ids_json(project_ids))
    .bind(ids_json(names))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// `None` when the collection doesn't exist.
pub async fn add_to_collection(pool: &Pool<Sqlite>, collection_id: &str, project_ids: &[String], now: &str) -> Result<Option<u64>, sqlx::Error> {
    if !collection_exists(pool, collection_id).await? {
        return Ok(None);
    }
    let res = sqlx::query(
        r#"
        INSERT OR IGNORE INTO collection_projects (collection_id, project_id, created_at)
        SELECT ?1, p.id, ?3 FROM projects p WHERE p.id IN (SELECT value FROM json_each(?2))
        "#,
    )
    .bind(collection_id)
    .bind(ids_json(project_ids))
    .bind(now)
    .execute(pool)
    .await?;
    touch(pool, collection_id, res.rows_affected(), now).await?;
    Ok(Some(res.rows_affected()))
}

/// `None` when the collection doesn't exist.
pub async fn remove_from_collection(pool: &Pool<Sqlite>, collection_id: &str, project_ids: &[String], now: &str) -> Result<Option<u64>, sqlx::Error> {
    if !collection_exists(pool, collection_id).await? {
        return Ok(None);
    }
    let res = sqlx::query("DELETE FROM collection_projects WHERE collection_id = ?1 AND project_id IN (SELECT value FROM json_each(?2))")
        .bind(collection_id)
        .bind(ids_json(project_ids))
        .execute(pool)
        .await?;
    touch(pool, collection_id, res.rows_affected(), now).await?;
    Ok(Some(res.rows_affected()))
}

async fn collection_exists(pool: &Pool<Sqlite>, id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM collections WHERE id = ?1").bind(id).fetch_one(pool).await? > 0)
}

async fn touch(pool: &Pool<Sqlite>, collection_id: &str, changed: u64, now: &str) -> Result<(), sqlx::Error> {
    if changed > 0 {
        sqlx::query("UPDATE collections SET updated_at = ?2 WHERE id = ?1").bind(collection_id).bind(now).execute(pool).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use uuid::Uuid;

    async fn project(pool: &Pool<Sqlite>, name: &str) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, 't', 't')")
            .bind(&id).bind(name).execute(pool).await.unwrap();
        id
    }

    async fn count(pool: &Pool<Sqlite>, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn tags_and_collections_for_many_projects() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();
        let ids = vec![project(pool, "a").await, project(pool, "b").await, "missing".to_string()];
        let other = project(pool, "c").await;

        assert_eq!(add_tags(pool, &ids, &["PLA".into(), "gift".into()], "t").await.unwrap(), 4, "2 tags × 2 real projects");
        assert_eq!(add_tags(pool, &ids, &["pla".into()], "t").await.unwrap(), 0, "same tag ignoring case, already there");
        assert_eq!(count(pool, "SELECT COUNT(*) FROM tags").await, 2);
        assert_eq!(remove_tags(pool, &ids, &["Gift".into()]).await.unwrap(), 2);
        assert_eq!(count(pool, "SELECT COUNT(*) FROM project_tags").await, 2);
        assert_eq!(count(pool, "SELECT COUNT(*) FROM tags").await, 2, "tags themselves stay");

        let c = crate::queries::collections::create_collection(pool, "Desk", "", "t").await.unwrap();
        crate::queries::collections::add_project(pool, &c.id, &other, "t").await.unwrap();
        assert_eq!(add_to_collection(pool, &c.id, &ids, "t2").await.unwrap(), Some(2));
        assert_eq!(add_to_collection(pool, &c.id, &ids, "t3").await.unwrap(), Some(0));
        assert_eq!(remove_from_collection(pool, &c.id, &ids, "t4").await.unwrap(), Some(2));
        assert_eq!(count(pool, "SELECT COUNT(*) FROM collection_projects").await, 1, "other project untouched");
        assert_eq!(add_to_collection(pool, "nope", &ids, "t").await.unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
