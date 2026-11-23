use sqlx::{Pool, Sqlite, FromRow};

#[derive(Debug, FromRow)]
pub struct ProjectDetailRow {
    pub id: String,
    pub folder_path: String,
    pub name: String,
    pub description: String,
    pub main_image_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_scanned_at: Option<String>,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_license: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct ProjectAssetRow {
    pub id: String,
    pub file_path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub file_hash: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct ProjectTagRow {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug)]
pub enum GetProjectError {
    NotFound,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for GetProjectError {
    fn from(e: sqlx::Error) -> Self { match e {
        sqlx::Error::RowNotFound => GetProjectError::NotFound,
        e => GetProjectError::Db(e)
    }}
}

impl std::fmt::Display for GetProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetProjectError::NotFound => write!(f, "Project not found"),
            GetProjectError::Db(e) => write!(f, "Database error: {}", e),
        }
    }
}

pub async fn get_project(
    pool: &Pool<Sqlite>,
    project_id: &str,
) -> Result<ProjectDetailRow, GetProjectError> {
    sqlx::query_as::<_, ProjectDetailRow>(
        r#"
        SELECT
            id,
            folder_path,
            name,
            description,
            main_image_id,
            created_at,
            updated_at,
            last_scanned_at,
            source_url,
            source_author,
            source_license
        FROM projects
        WHERE id = ?1
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(GetProjectError::from)
}

pub async fn get_project_tags(
    pool: &Pool<Sqlite>,
    project_id: &str,
) -> Result<Vec<ProjectTagRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectTagRow>(
        r#"
        SELECT
            t.id,
            t.name,
            t.color
        FROM tags t
        JOIN project_tags pt ON t.id = pt.tag_id
        WHERE pt.project_id = ?1
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

pub async fn get_project_assets(
    pool: &Pool<Sqlite>,
    project_id: &str,
) -> Result<Vec<ProjectAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectAssetRow>(
        r#"
        SELECT
            id,
            file_path,
            kind,
            size_bytes,
            file_hash
        FROM assets
        WHERE project_id = ?1
        ORDER BY CASE kind WHEN 'model' THEN 0 WHEN 'image' THEN 1 ELSE 2 END, file_path COLLATE NOCASE
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}