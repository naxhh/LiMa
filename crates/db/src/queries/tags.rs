use lima_domain::models::tag::default_tag_color;
use lima_domain::pagination::Cursor;
use sqlx::{FromRow, Pool, Sqlite, Transaction};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
    /// Projects using the tag.
    pub project_count: i64,
}

const TAG_COLUMNS: &str = "t.id, t.name, t.color, t.created_at, t.updated_at, \
    (SELECT COUNT(*) FROM project_tags pt WHERE pt.tag_id = t.id) AS project_count";

pub async fn list_tags(
    pool: &Pool<Sqlite>,
    limit: i64,
    cursor: Option<Cursor>,
) -> Result<Vec<TagRow>, sqlx::Error> {
    match cursor {
        None => { list_tags_from_start(pool, limit).await },
        Some(cursor) => { list_tags_from_cursor(pool, limit, &cursor.updated_at, &cursor.id).await },
    }
}

pub async fn list_tags_from_start(
    pool: &sqlx::Pool<Sqlite>,
    limit: i64,
) -> Result<Vec<TagRow>, sqlx::Error> {
    sqlx::query_as::<_, TagRow>(&format!(
        "SELECT {TAG_COLUMNS} FROM tags t ORDER BY t.updated_at DESC, t.id DESC LIMIT ?1"
    ))
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn list_tags_from_cursor(
    pool: &sqlx::Pool<Sqlite>,
    limit: i64,
    updated_at: &str,
    id: &str,
) -> Result<Vec<TagRow>, sqlx::Error> {
    sqlx::query_as::<_, TagRow>(&format!(
        "SELECT {TAG_COLUMNS} FROM tags t
         WHERE (t.updated_at, t.id) < (?1, ?2)
         ORDER BY t.updated_at DESC, t.id DESC LIMIT ?3"
    ))
    .bind(updated_at)
    .bind(id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Inserts a tag. `name` must already be normalized; `color` defaults to one
/// derived from the name.
pub async fn create_tag<'e, E>(
    ex: E,
    name: &str,
    color: Option<&str>,
    now: &str,
) -> Result<TagRow, sqlx::Error>
where E: sqlx::Executor<'e, Database = Sqlite>
{
    let id = Uuid::new_v4().to_string();
    let color = color.map(str::to_string).unwrap_or_else(|| default_tag_color(name));

    sqlx::query(
        r#"INSERT INTO tags (id, name, color, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)"#,
    )
    .bind(&id)
    .bind(name)
    .bind(&color)
    .bind(now)
    .execute(ex)
    .await?;
    
    Ok(TagRow {
        id: id.to_string(),
        name: name.to_string(),
        color: color.to_string(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
        project_count: 0,
    })
}

pub async fn get_tag(pool: &Pool<Sqlite>, id: &str) -> Result<Option<TagRow>, sqlx::Error> {
    sqlx::query_as::<_, TagRow>(&format!("SELECT {TAG_COLUMNS} FROM tags t WHERE t.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Id of a tag with this name, ignoring case.
pub async fn find_tag_id_by_name<'e, E>(ex: E, name: &str) -> Result<Option<String>, sqlx::Error>
where E: sqlx::Executor<'e, Database = Sqlite>
{
    sqlx::query_scalar::<_, String>(r#"SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE LIMIT 1"#)
        .bind(name)
        .fetch_optional(ex)
        .await
}

#[derive(Debug)]
pub enum UpdateTagError {
    NotFound,
    /// Another tag already uses the name (ignoring case).
    NameTaken,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for UpdateTagError {
    fn from(e: sqlx::Error) -> Self {
        UpdateTagError::Db(e)
    }
}

/// Renames and/or recolors a tag. Values must already be normalized.
pub async fn update_tag(
    pool: &Pool<Sqlite>,
    id: &str,
    name: Option<&str>,
    color: Option<&str>,
    now: &str,
) -> Result<TagRow, UpdateTagError> {
    let mut tx = pool.begin().await?;

    let exists: Option<String> = sqlx::query_scalar(r#"SELECT id FROM tags WHERE id = ?1"#)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(UpdateTagError::NotFound);
    }

    if let Some(name) = name {
        if let Some(other) = find_tag_id_by_name(&mut *tx, name).await?
            && other != id
        {
            return Err(UpdateTagError::NameTaken);
        }
        // Name update fires trg_tags_au, which refreshes the projects FTS index.
        sqlx::query(r#"UPDATE tags SET name = ?1, updated_at = ?2 WHERE id = ?3"#)
            .bind(name)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(color) = color {
        sqlx::query(r#"UPDATE tags SET color = ?1, updated_at = ?2 WHERE id = ?3"#)
            .bind(color)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    get_tag(pool, id).await?.ok_or(UpdateTagError::NotFound)
}

/// Deletes a tag and its project links. Returns false when it did not exist.
pub async fn delete_tag(pool: &Pool<Sqlite>, id: &str) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Remove links explicitly (rather than relying on the FK cascade) so the
    // project_tags delete trigger rebuilds the FTS rows of affected projects.
    sqlx::query(r#"DELETE FROM project_tags WHERE tag_id = ?1"#)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let res = sqlx::query(r#"DELETE FROM tags WHERE id = ?1"#).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Deletes several tags and their project links. Returns how many tags were deleted.
pub async fn delete_tags(pool: &Pool<Sqlite>, ids: &[String]) -> Result<i64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ids_json = serde_json::to_string(ids).expect("serialize ids");
    let mut tx = pool.begin().await?;
    // Remove links explicitly so the delete trigger rebuilds the FTS rows of affected projects.
    sqlx::query(r#"DELETE FROM project_tags WHERE tag_id IN (SELECT value FROM json_each(?1))"#)
        .bind(&ids_json)
        .execute(&mut *tx)
        .await?;
    let res = sqlx::query(r#"DELETE FROM tags WHERE id IN (SELECT value FROM json_each(?1))"#)
        .bind(&ids_json)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(res.rows_affected() as i64)
}

#[derive(Debug)]
pub enum MergeTagError {
    NotFound,
    /// `from_id` and `into_id` are the same tag.
    SameTag,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for MergeTagError {
    fn from(e: sqlx::Error) -> Self {
        MergeTagError::Db(e)
    }
}

/// Folds tag `from_id` into `into_id`: every project with `from_id` also gets
/// `into_id` (no duplicates), then `from_id` is deleted. The project_tags
/// insert/delete triggers keep the projects FTS index in sync, so `into_id`'s
/// projects become searchable and `from_id`'s no longer are. Returns the number
/// of projects that gained `into_id`.
pub async fn merge_tag(
    pool: &Pool<Sqlite>,
    from_id: &str,
    into_id: &str,
) -> Result<i64, MergeTagError> {
    if from_id == into_id {
        return Err(MergeTagError::SameTag);
    }

    let mut tx = pool.begin().await?;

    let ids: Vec<String> =
        sqlx::query_scalar(r#"SELECT id FROM tags WHERE id IN (?1, ?2)"#)
            .bind(from_id)
            .bind(into_id)
            .fetch_all(&mut *tx)
            .await?;
    if !ids.iter().any(|id| id == from_id) || !ids.iter().any(|id| id == into_id) {
        return Err(MergeTagError::NotFound);
    }

    let affected: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM project_tags WHERE tag_id = ?1"#)
        .bind(from_id)
        .fetch_one(&mut *tx)
        .await?;

    // Add the target tag to every project that has the source tag (duplicates ignored).
    sqlx::query(
        r#"INSERT OR IGNORE INTO project_tags (project_id, tag_id)
           SELECT project_id, ?1 FROM project_tags WHERE tag_id = ?2"#,
    )
    .bind(into_id)
    .bind(from_id)
    .execute(&mut *tx)
    .await?;

    // Remove the source tag's links explicitly so the delete trigger rebuilds FTS.
    sqlx::query(r#"DELETE FROM project_tags WHERE tag_id = ?1"#)
        .bind(from_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM tags WHERE id = ?1"#)
        .bind(from_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(affected)
}

// dont love this name. maybe get_tags_or_create or something?
pub async fn ensure_tags(
    tx: &mut Transaction<'_, Sqlite>,
    names: &[String],
    now: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let mut ids: Vec<String> = Vec::with_capacity(names.len());

    for name in names {
        // Reuse an existing tag even if the case differs ("PLA" vs "pla").
        let id = match find_tag_id_by_name(&mut **tx, name).await? {
            Some(id) => id,
            None => create_tag(&mut **tx, name, None, now).await?.id,
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    Ok(ids)
}

pub async fn set_project_tags(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    tag_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM project_tags WHERE project_id = ?1"#,
    )
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    for tag_id in tag_ids {
        sqlx::query(
            r#"INSERT OR IGNORE INTO project_tags (project_id, tag_id) VALUES (?1, ?2)"#,
        )
        .bind(project_id)
        .bind(tag_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    const NOW: &str = "2026-01-01T00:00:00Z";

    async fn db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        (db, dir)
    }

    async fn project(pool: &Pool<Sqlite>, name: &str, tags: &[&str]) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?3)")
            .bind(&id)
            .bind(name)
            .bind(NOW)
            .execute(pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let names: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let ids = ensure_tags(&mut tx, &names, NOW).await.unwrap();
        set_project_tags(&mut tx, &id, &ids).await.unwrap();
        tx.commit().await.unwrap();
        id
    }

    async fn search(pool: &Pool<Sqlite>, q: &str) -> Vec<String> {
        crate::queries::projects_search::search_projects(pool, q, 50, None, &Default::default())
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.project.name)
            .collect()
    }

    #[tokio::test]
    async fn ensure_tags_reuses_names_ignoring_case() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["PLA", "boats"]).await;
        project(pool, "dragon", &["pla", "pla"]).await;

        let tags = list_tags(pool, 50, None).await.unwrap();
        assert_eq!(tags.len(), 2, "{tags:?}");
        let pla = tags.iter().find(|t| t.name == "PLA").unwrap();
        assert_eq!(pla.project_count, 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn rename_recolor_and_conflicts() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats"]).await;
        project(pool, "dragon", &["fantasy"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();

        let renamed = update_tag(pool, &boats, Some("ships"), Some("#112233"), NOW).await.unwrap();
        assert_eq!((renamed.name.as_str(), renamed.color.as_str(), renamed.project_count), ("ships", "#112233", 1));
        // Search index follows the rename.
        assert_eq!(search(pool, "ships").await, ["benchy"]);
        assert!(search(pool, "boats").await.is_empty());

        // Case-only rename of itself is allowed; taking another tag's name is not.
        assert!(update_tag(pool, &boats, Some("Ships"), None, NOW).await.is_ok());
        assert!(matches!(update_tag(pool, &boats, Some("FANTASY"), None, NOW).await, Err(UpdateTagError::NameTaken)));
        assert!(matches!(update_tag(pool, "missing", Some("x"), None, NOW).await, Err(UpdateTagError::NotFound)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn delete_unlinks_projects_and_search() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats", "pla"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();
        assert_eq!(search(pool, "boats").await, ["benchy"]);

        assert!(delete_tag(pool, &boats).await.unwrap());
        assert!(!delete_tag(pool, &boats).await.unwrap());
        assert!(search(pool, "boats").await.is_empty());
        assert_eq!(search(pool, "pla").await, ["benchy"], "other tags stay searchable");
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_tags").fetch_one(pool).await.unwrap();
        assert_eq!(links, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn merge_moves_projects_and_search() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats"]).await;
        project(pool, "dragon", &["boats", "pla"]).await;
        project(pool, "unrelated", &["other"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();
        let ships = create_tag(pool, "ships", None, NOW).await.unwrap().id;

        assert_eq!(search(pool, "boats").await, ["benchy", "dragon"]);

        let affected = merge_tag(pool, &boats, &ships).await.unwrap();
        assert_eq!(affected, 2);
        // The source tag is gone; its projects moved over.
        assert!(find_tag_id_by_name(pool, "boats").await.unwrap().is_none());
        assert_eq!(search(pool, "boats").await, Vec::<String>::new());
        assert_eq!(search(pool, "ships").await, ["benchy", "dragon"]);
        assert_eq!(search(pool, "pla").await, ["dragon"], "untouched tag stays searchable");
        // No duplicate links for the project that already had both.
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_tags WHERE project_id = (SELECT id FROM projects WHERE name = 'dragon')")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(links, 2, "ships + pla");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn merge_into_tag_project_already_has() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats", "ships"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();
        let ships = find_tag_id_by_name(pool, "ships").await.unwrap().unwrap();

        let affected = merge_tag(pool, &boats, &ships).await.unwrap();
        assert_eq!(affected, 1, "the project gained nothing new but still counts");
        assert!(find_tag_id_by_name(pool, "boats").await.unwrap().is_none());
        assert_eq!(search(pool, "ships").await, ["benchy"]);
        assert!(search(pool, "boats").await.is_empty());
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_tags").fetch_one(pool).await.unwrap();
        assert_eq!(links, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn merge_errors() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();

        assert!(matches!(merge_tag(pool, "missing", &boats).await, Err(MergeTagError::NotFound)));
        assert!(matches!(merge_tag(pool, &boats, "missing").await, Err(MergeTagError::NotFound)));
        assert!(matches!(merge_tag(pool, &boats, &boats).await, Err(MergeTagError::SameTag)));
        // The failed merges left the tag alone.
        assert_eq!(search(pool, "boats").await, ["benchy"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn bulk_delete_removes_links_and_search() {
        let (db, dir) = db().await;
        let pool = db.pool();
        project(pool, "benchy", &["boats", "pla"]).await;
        project(pool, "dragon", &["pla"]).await;
        let boats = find_tag_id_by_name(pool, "boats").await.unwrap().unwrap();
        let pla = find_tag_id_by_name(pool, "pla").await.unwrap().unwrap();

        let deleted = delete_tags(pool, &[boats.clone(), pla.clone(), "missing".to_string()]).await.unwrap();
        assert_eq!(deleted, 2, "only real tags count");
        assert!(search(pool, "boats").await.is_empty());
        assert!(search(pool, "pla").await.is_empty());
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_tags").fetch_one(pool).await.unwrap();
        assert_eq!(links, 0);
        // Deleting nothing deletes nothing.
        assert_eq!(delete_tags(pool, &[]).await.unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
