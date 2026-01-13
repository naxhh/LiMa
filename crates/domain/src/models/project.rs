use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct ProjectRow {
    pub id: String,
    pub folder_path: String,
    pub name: String,
    pub description: String,
    pub main_image_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_scanned_at: Option<String>,
}
