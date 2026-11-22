//! Collections: named, hand-picked groups of projects.

use std::collections::HashMap;

use lima_domain::models::project::ProjectRow;
use sqlx::{FromRow, Pool, Sqlite};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct CollectionRow {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Set for built-in collections (`favourites`), `None` for the user's own.
    pub system_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub project_count: i64,
}

const COLUMNS: &str = r#"
    c.id, c.name, c.description, c.system_key, c.created_at, c.updated_at,
    (SELECT COUNT(*) FROM collection_projects cp WHERE cp.collection_id = c.id) AS project_count
"#;

/// A project image used to preview a collection.
#[derive(Debug, Clone, FromRow)]
pub struct CoverImage {
    pub project_id: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct CollectionRef {
    pub id: String,
    pub name: String,
}

pub async fn list_collections(pool: &Pool<Sqlite>) -> Result<Vec<CollectionRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionRow>(&format!(
        "SELECT {COLUMNS} FROM collections c ORDER BY c.system_key IS NULL, c.name COLLATE NOCASE, c.id"
    ))
    .fetch_all(pool)
    .await
}

pub async fn get_collection(pool: &Pool<Sqlite>, id: &str) -> Result<Option<CollectionRow>, sqlx::Error> {
    sqlx::query_as::<_, CollectionRow>(&format!("SELECT {COLUMNS} FROM collections c WHERE c.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Id of the collection with this name, ignoring case.
pub async fn find_collection_id_by_name(pool: &Pool<Sqlite>, name: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT id FROM collections WHERE name = ?1 COLLATE NOCASE LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Up to `per_collection` cover images per collection, most recently added projects first.
/// Only projects with a main image count.
pub async fn cover_images(
    pool: &Pool<Sqlite>,
    collection_ids: &[String],
    per_collection: i64,
) -> Result<HashMap<String, Vec<CoverImage>>, sqlx::Error> {
    let mut out: HashMap<String, Vec<CoverImage>> = HashMap::new();
    if collection_ids.is_empty() {
        return Ok(out);
    }
    #[derive(FromRow)]
    struct Row {
        collection_id: String,
        project_id: String,
        asset_id: String,
    }
    let ids_json = serde_json::to_string(collection_ids).expect("serialize ids");
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT collection_id, project_id, asset_id FROM (
            SELECT cp.collection_id, p.id AS project_id, p.main_image_id AS asset_id,
                   ROW_NUMBER() OVER (PARTITION BY cp.collection_id ORDER BY cp.created_at DESC, p.id) AS n
            FROM collection_projects cp
            JOIN projects p ON p.id = cp.project_id
            WHERE p.main_image_id IS NOT NULL
              AND cp.collection_id IN (SELECT value FROM json_each(?1))
        )
        WHERE n <= ?2
        ORDER BY collection_id, n
        "#,
    )
    .bind(&ids_json)
    .bind(per_collection)
    .fetch_all(pool)
    .await?;
    for r in rows {
        out.entry(r.collection_id).or_default().push(CoverImage { project_id: r.project_id, asset_id: r.asset_id });
    }
    Ok(out)
}

/// The collection's projects in `sort` order; `None` sorts by when they were added to the collection.
pub async fn collection_projects(
    pool: &Pool<Sqlite>,
    collection_id: &str,
    sort: Option<crate::queries::projects::ProjectSort>,
    descending: bool,
) -> Result<Vec<ProjectRow>, sqlx::Error> {
    // Only fixed strings go into the SQL.
    let expr = sort.map_or("cp.created_at", |s| s.sql_expr());
    let dir = if descending { "DESC" } else { "ASC" };
    sqlx::query_as::<_, ProjectRow>(&format!(
        r#"
        SELECT p.id, p.folder_path, p.name, p.description, p.main_image_id, p.created_at, p.updated_at, p.last_scanned_at
        FROM collection_projects cp
        JOIN projects p ON p.id = cp.project_id
        WHERE cp.collection_id = ?1
        ORDER BY {expr} {dir}, p.id {dir}
        "#
    ))
    .bind(collection_id)
    .fetch_all(pool)
    .await
}

/// Collections a project belongs to, by name.
pub async fn collections_for_project(pool: &Pool<Sqlite>, project_id: &str) -> Result<Vec<CollectionRef>, sqlx::Error> {
    sqlx::query_as::<_, CollectionRef>(
        r#"
        SELECT c.id, c.name
        FROM collection_projects cp
        JOIN collections c ON c.id = cp.collection_id
        WHERE cp.project_id = ?1 AND c.system_key IS NULL
        ORDER BY c.name COLLATE NOCASE
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

pub async fn create_collection(
    pool: &Pool<Sqlite>,
    name: &str,
    description: &str,
    now: &str,
) -> Result<CollectionRow, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO collections (id, name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)")
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(CollectionRow {
        id,
        name: name.to_string(),
        description: description.to_string(),
        system_key: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
        project_count: 0,
    })
}

/// Updates the given fields. Returns false when the collection doesn't exist.
pub async fn update_collection(
    pool: &Pool<Sqlite>,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    now: &str,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        r#"
        UPDATE collections
        SET name = COALESCE(?2, name), description = COALESCE(?3, description), updated_at = ?4
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Deletes the collection and its memberships (projects are untouched).
pub async fn delete_collection(pool: &Pool<Sqlite>, id: &str) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM collection_projects WHERE collection_id = ?1").bind(id).execute(&mut *tx).await?;
    let res = sqlx::query("DELETE FROM collections WHERE id = ?1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Key of the built-in Favourites collection.
pub const FAVOURITES_KEY: &str = "favourites";
const FAVOURITES_NAME: &str = "Favourites";

/// Id of the Favourites collection, creating it when missing. Safe to call on every startup.
pub async fn ensure_favourites(pool: &Pool<Sqlite>, now: &str) -> Result<String, sqlx::Error> {
    if let Some(id) = favourites_id(pool).await? {
        return Ok(id);
    }
    // A user collection with that name becomes the built-in one instead of clashing.
    if let Some(id) = find_collection_id_by_name(pool, FAVOURITES_NAME).await? {
        sqlx::query("UPDATE collections SET system_key = ?2 WHERE id = ?1").bind(&id).bind(FAVOURITES_KEY).execute(pool).await?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO collections (id, name, description, system_key, created_at, updated_at) VALUES (?1, ?2, '', ?3, ?4, ?4)",
    )
    .bind(&id)
    .bind(FAVOURITES_NAME)
    .bind(FAVOURITES_KEY)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn is_favourite(pool: &Pool<Sqlite>, project_id: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM collection_projects cp JOIN collections c ON c.id = cp.collection_id
            WHERE cp.project_id = ?1 AND c.system_key = ?2
        )
        "#,
    )
    .bind(project_id)
    .bind(FAVOURITES_KEY)
    .fetch_one(pool)
    .await
}

pub async fn favourites_id(pool: &Pool<Sqlite>) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT id FROM collections WHERE system_key = ?1")
        .bind(FAVOURITES_KEY)
        .fetch_optional(pool)
        .await
}

#[derive(Debug, PartialEq, Eq)]
pub enum MembershipError {
    CollectionNotFound,
    ProjectNotFound,
    Db(String),
}

impl From<sqlx::Error> for MembershipError {
    fn from(e: sqlx::Error) -> Self {
        MembershipError::Db(e.to_string())
    }
}

async fn ensure_both_exist(pool: &Pool<Sqlite>, collection_id: &str, project_id: &str) -> Result<(), MembershipError> {
    let (c, p): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM collections WHERE id = ?1), (SELECT COUNT(*) FROM projects WHERE id = ?2)",
    )
    .bind(collection_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    if c == 0 {
        return Err(MembershipError::CollectionNotFound);
    }
    if p == 0 {
        return Err(MembershipError::ProjectNotFound);
    }
    Ok(())
}

/// Adds a project. Adding one that is already there is a no-op. Returns true when it was added.
pub async fn add_project(
    pool: &Pool<Sqlite>,
    collection_id: &str,
    project_id: &str,
    now: &str,
) -> Result<bool, MembershipError> {
    ensure_both_exist(pool, collection_id, project_id).await?;
    let res = sqlx::query(
        "INSERT OR IGNORE INTO collection_projects (collection_id, project_id, created_at) VALUES (?1, ?2, ?3)",
    )
    .bind(collection_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    let added = res.rows_affected() > 0;
    if added {
        touch(pool, collection_id, now).await?;
    }
    Ok(added)
}

/// Removes a project. Returns true when it was in the collection.
pub async fn remove_project(
    pool: &Pool<Sqlite>,
    collection_id: &str,
    project_id: &str,
    now: &str,
) -> Result<bool, MembershipError> {
    ensure_both_exist(pool, collection_id, project_id).await?;
    let res = sqlx::query("DELETE FROM collection_projects WHERE collection_id = ?1 AND project_id = ?2")
        .bind(collection_id)
        .bind(project_id)
        .execute(pool)
        .await?;
    let removed = res.rows_affected() > 0;
    if removed {
        touch(pool, collection_id, now).await?;
    }
    Ok(removed)
}

async fn touch(pool: &Pool<Sqlite>, collection_id: &str, now: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE collections SET updated_at = ?2 WHERE id = ?1")
        .bind(collection_id)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    async fn db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        (db, dir)
    }

    async fn project(pool: &Pool<Sqlite>, name: &str, with_image: bool) -> String {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?3)")
            .bind(&id).bind(name).bind("2026-01-01T00:00:00Z").execute(pool).await.unwrap();
        if with_image {
            let asset = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, created_at, updated_at) VALUES (?1, ?2, 'a.jpg', 'image', 1, ?3, ?3, ?3)")
                .bind(&asset).bind(&id).bind("2026-01-01T00:00:00Z").execute(pool).await.unwrap();
            sqlx::query("UPDATE projects SET main_image_id = ?1 WHERE id = ?2").bind(&asset).bind(&id).execute(pool).await.unwrap();
        }
        id
    }

    #[tokio::test]
    async fn favourites_is_created_once_and_listed_first() {
        let (db, dir) = db().await;
        let pool = db.pool();
        create_collection(pool, "Aardvarks", "", "t").await.unwrap();
        let fav = ensure_favourites(pool, "t").await.unwrap();
        assert_eq!(ensure_favourites(pool, "t").await.unwrap(), fav, "idempotent");
        let list = list_collections(pool).await.unwrap();
        assert_eq!(list[0].id, fav);
        assert_eq!(list[0].system_key.as_deref(), Some(FAVOURITES_KEY));
        let p = project(pool, "p", false).await;
        add_project(pool, &fav, &p, "t").await.unwrap();
        assert!(collections_for_project(pool, &p).await.unwrap().is_empty(), "favourites isn't listed as a regular membership");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn existing_favourites_collection_is_adopted() {
        let (db, dir) = db().await;
        let pool = db.pool();
        let mine = create_collection(pool, "favourites", "", "t").await.unwrap();
        assert_eq!(ensure_favourites(pool, "t").await.unwrap(), mine.id);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn membership_counts_covers_and_cleanup() {
        let (db, dir) = db().await;
        let pool = db.pool();
        let c = create_collection(pool, "Desk toys", "", "2026-01-01T00:00:00Z").await.unwrap();
        let (a, b, x) = (project(pool, "a", true).await, project(pool, "b", false).await, project(pool, "x", true).await);

        assert!(add_project(pool, &c.id, &a, "2026-01-02T00:00:00Z").await.unwrap());
        assert!(add_project(pool, &c.id, &b, "2026-01-03T00:00:00Z").await.unwrap());
        assert!(!add_project(pool, &c.id, &a, "2026-01-04T00:00:00Z").await.unwrap(), "re-adding is a no-op");
        assert_eq!(add_project(pool, &c.id, "nope", "t").await, Err(MembershipError::ProjectNotFound));
        assert_eq!(add_project(pool, "nope", &a, "t").await, Err(MembershipError::CollectionNotFound));

        let row = get_collection(pool, &c.id).await.unwrap().unwrap();
        assert_eq!(row.project_count, 2);
        assert_eq!(row.updated_at, "2026-01-03T00:00:00Z");
        let names: Vec<_> = collection_projects(pool, &c.id, None, true).await.unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["b", "a"], "newest first");
        let cid = c.id.clone();
        let sorted = |sort, desc| {
            let cid = cid.clone();
            async move { collection_projects(pool, &cid, sort, desc).await.unwrap().into_iter().map(|p| p.name).collect::<Vec<_>>() }
        };
        use crate::queries::projects::ProjectSort;
        assert_eq!(sorted(None, false).await, vec!["a", "b"], "first added first");
        assert_eq!(sorted(Some(ProjectSort::Name), false).await, vec!["a", "b"]);
        assert_eq!(sorted(Some(ProjectSort::Name), true).await, vec!["b", "a"]);

        // Only projects with an image give covers.
        let covers = cover_images(pool, &[c.id.clone()], 4).await.unwrap();
        assert_eq!(covers[&c.id].len(), 1);
        assert_eq!(covers[&c.id][0].project_id, a);

        assert_eq!(collections_for_project(pool, &a).await.unwrap().len(), 1);
        assert!(collections_for_project(pool, &x).await.unwrap().is_empty());
        assert_eq!(find_collection_id_by_name(pool, "DESK TOYS").await.unwrap(), Some(c.id.clone()));

        assert!(remove_project(pool, &c.id, &a, "2026-01-05T00:00:00Z").await.unwrap());
        assert!(!remove_project(pool, &c.id, &a, "2026-01-06T00:00:00Z").await.unwrap());

        // Deleting the project drops its membership; deleting the collection keeps projects.
        sqlx::query("DELETE FROM projects WHERE id = ?1").bind(&b).execute(pool).await.unwrap();
        assert_eq!(get_collection(pool, &c.id).await.unwrap().unwrap().project_count, 0);
        add_project(pool, &c.id, &x, "t").await.unwrap();
        assert!(delete_collection(pool, &c.id).await.unwrap());
        assert!(get_collection(pool, &c.id).await.unwrap().is_none());
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects").fetch_one(pool).await.unwrap();
        assert_eq!(left, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
