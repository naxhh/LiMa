use std::path::PathBuf;

use sqlx::{Pool, Sqlite, Transaction};
use uuid::Uuid;

pub struct InsertedAsset {
    pub id: String,
    pub file_path: String,
    pub kind: String,
}

pub async fn insert_asset(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    file_path: &str,
    kind: &str,
    size_bytes: i64,
    mtime: &str,
    mime: &str,
    now: &str,
) -> Result<InsertedAsset, sqlx::Error> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"INSERT INTO assets (id, project_id, file_path, kind, size_bytes, mtime, mime, file_hash, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8)"#,
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

    Ok(InsertedAsset {
        id,
        file_path: file_path.to_string(),
        kind: kind.to_string(),
    })
}

pub enum DeleteAssetError {
    NotFound { project_id: String },
    Db(sqlx::Error),
    Fs(std::io::Error),
}

impl ToString for DeleteAssetError {
    fn to_string(&self) -> String {
        match self {
            DeleteAssetError::NotFound { project_id } => format!("Asset not found in project {}", project_id),
            DeleteAssetError::Db(e) => format!("Database error: {}", e),
            DeleteAssetError::Fs(e) => format!("Filesystem error: {}", e),
        }
    }
}

pub async fn delete_asset(
    pool: &Pool<Sqlite>,
    project_id: &str,
    asset_id: &str,
) -> Result<(), DeleteAssetError> {
    let mut tx = pool.begin().await.map_err(DeleteAssetError::Db)?;

    // todo: we may want to store the full project/file path to avoid needing to do this.
    let project_path = match sqlx::query_scalar::<_, String>(
        r#"SELECT folder_path FROM projects WHERE id = ?1"#,
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await {
        Ok(path) => path,
        // todo: we keep a generic error since we want to remove this with above todo
        Err(sqlx::Error::RowNotFound) => {
            return Err(DeleteAssetError::NotFound { project_id: project_id.to_string() });
        }
        Err(e) => return Err(DeleteAssetError::Db(e))
    };

    let file_path: String = match sqlx::query_scalar::<_, String>(
        r#"SELECT file_path FROM assets WHERE id = ?1 AND project_id = ?2"#,
    )
    .bind(asset_id)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await {
        Ok(path) => path,
        Err(sqlx::Error::RowNotFound) => {
            return Err(DeleteAssetError::NotFound { project_id: project_id.to_string() });
        }
        Err(e) => return Err(DeleteAssetError::Db(e))
    };

    let result = sqlx::query(
        r#"DELETE FROM assets WHERE id = ?1 AND project_id = ?2"#,
    )
    .bind(asset_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await
    .map_err(DeleteAssetError::Db)?;

    if result.rows_affected() == 0 {
        return Err(DeleteAssetError::NotFound { project_id: project_id.to_string() });
    }

    // delete from disk
    let full_asset_path: PathBuf = ["data", "library", &project_path, &file_path].iter().collect();
    if let Err(e) = tokio::fs::remove_file(&full_asset_path).await {
        tracing::error!("Failed to delete asset file at {:?}: {}", full_asset_path, e);
        tx.rollback().await.map_err(DeleteAssetError::Db)?;
        return Err(DeleteAssetError::Fs(e));
    }

    tx.commit().await.map_err(DeleteAssetError::Db)?;

    Ok(())
}

pub async fn set_project_main_image(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    asset_id: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE projects SET main_image_id = ?1, updated_at = ?2 WHERE id = ?3"#,
    )
    .bind(asset_id)
    .bind(now)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}