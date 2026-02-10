use sqlx::Pool;
use tokio::fs;
use std::path::PathBuf;
use lima_domain::models::bundle::BundleMeta;
use serde_json;

use crate::queries::projects_detail::ProjectAssetRow;

#[derive(Debug)]
pub enum ImportFromBundleError {
    BundleNotFound,
    MetaNotFound,
    ProjectNotFound,
    MissingFile { name: String },
    Conflict { name: String },
    FileSystemError(String),
    DatabaseError(sqlx::Error),
    
}

/*impl From<sqlx::Error> for ImportFromBundleError {
    fn from(err: sqlx::Error) -> Self {
        sqlx::Error::RowNotFound => ImportFromBundleError::BundleNotFound,
        e => ImportFromBundleError::DatabaseError(err)
    }
}*/

impl ToString for ImportFromBundleError {
    fn to_string(&self) -> String {
        match self {
            ImportFromBundleError::BundleNotFound => "Bundle not found".to_string(),
            ImportFromBundleError::MetaNotFound => "Missing meta file in bundle".to_string(),
            ImportFromBundleError::ProjectNotFound => "Project not found".to_string(),
            ImportFromBundleError::MissingFile { name } => format!("Missing file in bundle: {}", name),
            ImportFromBundleError::Conflict { name } => format!("Conflict with existing file: {}", name),
            ImportFromBundleError::DatabaseError(e) => format!("Database error: {}", e),
            ImportFromBundleError::FileSystemError(e) => format!("File system error: {}", e),
        }
    }
}

pub async fn import_assets_from_bundle(
    pool: &Pool<sqlx::Sqlite>,
    project_id: &str,
    bundle_id: &str,
) -> Result<Vec<ProjectAssetRow>, ImportFromBundleError> {
    let bundle_folder: PathBuf = ["data", "state", "bundles", &bundle_id].iter().collect();

    if !fs::metadata(&bundle_folder).await.is_ok() {
        return Err(ImportFromBundleError::BundleNotFound);
    }

    let meta = match get_bundle_meta_file(&bundle_folder).await {
        Some(meta) => meta,
        None => return Err(ImportFromBundleError::MetaNotFound),
    };

    if meta.files.is_empty() {
        fs::remove_dir_all(&bundle_folder).await.ok();
        return Ok(vec![]);
    }

    let project  = match crate::queries::projects_detail::get_project(pool, project_id).await {
        Ok(proj) => proj,
        Err(crate::queries::projects_detail::GetProjectError::NotFound) => { return Err(ImportFromBundleError::ProjectNotFound); },
        Err(_) => return Err(ImportFromBundleError::DatabaseError(sqlx::Error::RowNotFound)),
    };
    
    // We assume if project exists folder structure is valid.
    let project_dir: PathBuf = ["data", "library", &project.folder_path].iter().collect();
    let mut moved_files: Vec<PathBuf> = Vec::with_capacity(meta.files.len());
    let mut built_assets: Vec<ProjectAssetRow> = Vec::with_capacity(meta.files.len());

    let mut transaction = pool.begin().await.map_err(ImportFromBundleError::DatabaseError)?;

    for file_info in &meta.files {
        // check if file exists in bundle
        let src = bundle_folder.join(&file_info.name);
        if fs::metadata(&src).await.is_err() {
            rollback_imported_files(&moved_files).await;
            let _ = transaction.rollback().await;
            return Err(ImportFromBundleError::MissingFile { name: file_info.name.clone() });
        }

        // Ensure we don't replace existing files. We want to respect what is in the folders already.
        let dst = project_dir.join(&file_info.name);
        if fs::metadata(&dst).await.is_ok() {
            rollback_imported_files(&moved_files).await;
            let _ = transaction.rollback().await;
            return Err(ImportFromBundleError::Conflict { name: file_info.name.clone() } );
        }

        if let Err(e) = move_file(&src, &dst).await {
            rollback_imported_files(&moved_files).await;
            let _ = transaction.rollback().await;
            return Err(ImportFromBundleError::FileSystemError(e.to_string()));
        }

        moved_files.push(dst);

        let asset_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
        r#"
            INSERT INTO assets (
              id, project_id, file_path, kind, size_bytes, mtime, mime, file_hash, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(project_id, file_path) DO UPDATE SET
              kind = excluded.kind,
              size_bytes = excluded.size_bytes,
              mtime = excluded.mtime,
              mime = excluded.mime,
              file_hash = excluded.file_hash,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(&asset_id)
        .bind(&project.id)
        .bind(&file_info.name)
        .bind(&file_info.kind)
        .bind(file_info.size)
        .bind(file_info.mtime.as_deref().unwrap_or(""))
        .bind(&file_info.mime)
        .bind(&file_info.checksum.as_deref())
        .bind(&meta.uploaded_at)
        .bind(&meta.uploaded_at)
        .execute(&mut *transaction)
        .await
        .map_err(ImportFromBundleError::DatabaseError)?;

        built_assets.push(ProjectAssetRow {
            id: asset_id,
            file_path: file_info.name.clone(),
            kind: file_info.kind.clone(),
            size_bytes: file_info.size,
        });
    };

    if let Err(e) = transaction.commit().await {
        rollback_imported_files(&moved_files).await;
        return Err(ImportFromBundleError::DatabaseError(e));
    }

    // Ignore the error. Files have been imported correctly and the cron job will take care of cleaning up if needed.
    let _ = fs::remove_dir_all(&bundle_folder)
        .await
        .ok();

    return Ok(built_assets);
}

async fn get_bundle_meta_file(bundle_folder: &PathBuf) -> Option<BundleMeta> {
    let meta_path = bundle_folder.join("meta.json");
    let meta_data = fs::read_to_string(meta_path).await.ok()?;
    let bundle_meta: BundleMeta = serde_json::from_str(&meta_data).ok()?;
    Some(bundle_meta)
}

async fn move_file(src: &PathBuf, dst: &PathBuf) -> Result<(), std::io::Error> {
    match fs::rename(src, dst).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            fs::copy(src, dst).await?;
            fs::remove_file(src).await?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

async fn rollback_imported_files(moved_files: &Vec<PathBuf>) {
    for file_path in moved_files.iter() {
        let _ = fs::remove_file(file_path).await;
    }
}
