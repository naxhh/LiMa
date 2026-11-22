use sqlx::{Pool, Sqlite};

/// Records where an imported project came from.
pub async fn set_project_source(
    pool: &Pool<Sqlite>,
    project_id: &str,
    url: &str,
    author: Option<&str>,
    license: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE projects SET source_url = ?2, source_author = ?3, source_license = ?4 WHERE id = ?1"#)
        .bind(project_id)
        .bind(url)
        .bind(author)
        .bind(license)
        .execute(pool)
        .await?;
    Ok(())
}
