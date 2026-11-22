use lima_domain::config::Paths;
use sqlx::{Pool, Sqlite};

#[derive(Debug)]
pub enum DeleteProjectError {
    NotFound,
    DeleteFailed(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for DeleteProjectError {
    fn from(err: sqlx::Error) -> Self {
        DeleteProjectError::Db(err)
    }
}

pub async fn delete_project(
    pool: &Pool<Sqlite>,
    paths: &Paths,
    project_id: &str,
) -> Result<(), DeleteProjectError> {
    let mut tx = pool.begin().await?;

    let folder_path: Option<String> = sqlx::query_scalar(
        r#"SELECT folder_path FROM projects WHERE id = ?1"#,
    )
    .bind(project_id)
    .fetch_optional(&mut *tx)
    .await?;

    let folder_path = match folder_path {
        Some(path) => path,
        None => return Err(DeleteProjectError::NotFound),
    };

    let project_dir = paths.project_dir(&folder_path);
    tokio::fs::remove_dir_all(&project_dir).await.map_err(|e| {
        DeleteProjectError::DeleteFailed(format!("Failed to delete project directory: {}", e))
    })?;

    sqlx::query(
        r#"DELETE FROM projects WHERE id = ?1"#,
    )
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(())
}

/// Removes the project from the index but leaves its folder on disk, and adds the
/// folder to the ignore list so the next sync doesn't bring it back.
pub async fn remove_project_keep_files(pool: &Pool<Sqlite>, project_id: &str, now: &str) -> Result<(), DeleteProjectError> {
    let mut tx = pool.begin().await?;
    let folder_path: String = sqlx::query_scalar("SELECT folder_path FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DeleteProjectError::NotFound)?;
    sqlx::query("INSERT OR IGNORE INTO ignored_folders (folder_path, created_at) VALUES (?1, ?2)")
        .bind(&folder_path)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM projects WHERE id = ?1").bind(project_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use uuid::Uuid;

    #[tokio::test]
    async fn remove_keep_files_ignores_the_folder_and_leaves_files() {
        let dir = std::env::temp_dir().join(format!("lima-db-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("library/benchy")).unwrap();
        std::fs::write(dir.join("library/benchy/benchy.stl"), "solid").unwrap();
        let db = Db::connect(&format!("sqlite:{}?mode=rwc", dir.join("t.db").display())).await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects (id, folder_path, name, created_at, updated_at) VALUES (?1, 'benchy', 'benchy', 't', 't')")
            .bind(&id).execute(pool).await.unwrap();

        remove_project_keep_files(pool, &id, "2026-11-22T00:00:00Z").await.unwrap();

        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects").fetch_one(pool).await.unwrap();
        assert_eq!(left, 0);
        assert!(dir.join("library/benchy/benchy.stl").exists(), "files must stay on disk");
        assert!(crate::queries::ignored_folders::ignored_set(pool).await.unwrap().contains("benchy"));
        assert!(matches!(remove_project_keep_files(pool, &id, "t").await, Err(DeleteProjectError::NotFound)));

        assert!(crate::queries::ignored_folders::unignore(pool, "benchy").await.unwrap());
        assert!(!crate::queries::ignored_folders::unignore(pool, "benchy").await.unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
