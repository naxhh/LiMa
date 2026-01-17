use std::path::PathBuf;
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

    let project_dir: PathBuf = ["data", "library", &folder_path].iter().collect();
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
