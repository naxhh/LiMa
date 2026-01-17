use sqlx::{Sqlite, Transaction};
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