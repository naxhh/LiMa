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
}

#[derive(Debug, FromRow)]
pub struct ProjectAssetRow {
    pub id: String,
    pub file_path: String,
    pub kind: String,
    pub size_bytes: i64,
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

impl ToString for GetProjectError {
    fn to_string(&self) -> String {
        match self {
            GetProjectError::NotFound => "Project not found".to_string(),
            GetProjectError::Db(e) => format!("Database error: {}", e),
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
            last_scanned_at
        FROM projects
        WHERE id = ?1
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|e| GetProjectError::from(e))
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
            size_bytes
        FROM assets
        WHERE project_id = ?1
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}