use sqlx::{Pool, Sqlite};
use uuid::Uuid;

pub struct CreatedProject {
    pub id: String,
    pub folder_path: String,
}

pub async fn create_project(
    pool: &Pool<Sqlite>,
    name: &str,
    description: &str,
    main_image_id: Option<&str>,
    tags: &[String],
    now: &str,
) -> Result<CreatedProject, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let project_id = Uuid::new_v4().to_string();
    let folder_path = slugify_string(name);

    sqlx::query(
        r#"INSERT INTO projects (id, folder_path, name, description, main_image_id, created_at, updated_at, last_scanned_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL)"#,
    )
    .bind(&project_id)
    .bind(&folder_path)
    .bind(name)
    .bind(description)
    .bind(main_image_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;


    if !tags.is_empty() {
        let tag_ids = crate::queries::tags::ensure_tags(&mut tx, tags, now).await?;
        crate::queries::tags::set_project_tags(&mut tx, &project_id, &tag_ids).await?;
    }

    tx.commit().await?;

    Ok(CreatedProject {
        id: project_id,
        folder_path: folder_path,
    })
}

fn slugify_string(input: &str) -> String {
    let out = input
        .trim()
        .to_lowercase()
        .replace(" ", "-")
        .replace("/", "-")
        .replace("\0", "-")
        .replace(".", "-")
        .replace("..", "-");
        

    out.trim_matches('-').to_string()
}