use sqlx::{Pool, Sqlite, Transaction};

pub async fn update_project(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    main_image_id: Option<&str>,
    now: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE projects
        SET name = COALESCE(?1, name),
            description = COALESCE(?2, description),
            main_image_id = COALESCE(?3, main_image_id),
            updated_at = ?4
        WHERE id = ?5
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(main_image_id)
    .bind(now)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    Ok(result.rows_affected())
}

pub async fn set_main_image(
    pool: &Pool<Sqlite>,
    project_id: &str,
    main_image_id: &str,
    now: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE projects
        SET main_image_id = ?1,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(main_image_id)
    .bind(now)
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
