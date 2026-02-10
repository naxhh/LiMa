use lima_domain::pagination::Cursor;
use sqlx::{FromRow, Pool, Sqlite, Transaction};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_tags(
    pool: &Pool<Sqlite>,
    limit: i64,
    cursor: Option<Cursor>,
) -> Result<Vec<TagRow>, sqlx::Error> {
    match cursor {
        None => { list_tags_from_start(pool, limit).await },
        Some(cursor) => { list_tags_from_cursor(pool, limit, &cursor.updated_at, &cursor.id).await },
    }
}

pub async fn list_tags_from_start(
    pool: &sqlx::Pool<Sqlite>,
    limit: i64,
) -> Result<Vec<TagRow>, sqlx::Error> {
    sqlx::query_as::<_, TagRow>(
        r#"
        SELECT id, name, color, created_at, updated_at
        FROM tags
        ORDER BY updated_at DESC, id DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn list_tags_from_cursor(
    pool: &sqlx::Pool<Sqlite>,
    limit: i64,
    updated_at: &str,
    id: &str,
) -> Result<Vec<TagRow>, sqlx::Error> {
    sqlx::query_as::<_, TagRow>(
        r#"
        SELECT id, name, color, created_at, updated_at
        FROM tags
        WHERE (updated_at, id) < (?1, ?2)
        ORDER BY updated_at DESC, id DESC
        LIMIT ?3
        "#,
    )
    .bind(updated_at)
    .bind(id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn create_tag<'e, E>(
    ex: E,
    name: &str,
    now: &str,
) -> Result<TagRow, sqlx::Error>
where E: sqlx::Executor<'e, Database = Sqlite>
{
    let id = Uuid::new_v4().to_string();
    let color = string_to_hex_color(name);

    sqlx::query(
        r#"INSERT INTO tags (id, name, color, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)"#,
    )
    .bind(&id)
    .bind(name)
    .bind(&color)
    .bind(now)
    .execute(ex)
    .await?;
    
    Ok(TagRow {
        id: id.to_string(),
        name: name.to_string(),
        color: color.to_string(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

// dont love this name. maybe get_tags_or_create or something?
pub async fn ensure_tags(
    tx: &mut Transaction<'_, Sqlite>,
    names: &[String],
    now: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let mut ids = Vec::with_capacity(names.len());

    for name in names {
        let existing = sqlx::query_scalar::<_, String>(
            r#"SELECT id FROM tags WHERE name = ?"#,
        )
        .bind(name)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(id) = existing {
            ids.push(id);
            continue;
        }

        let tag = create_tag(&mut **tx, name, now).await?;
        ids.push(tag.id);
    }

    Ok(ids)
}

pub async fn set_project_tags(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    tag_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM project_tags WHERE project_id = ?1"#,
    )
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    for tag_id in tag_ids {
        sqlx::query(
            r#"INSERT OR IGNORE INTO project_tags (project_id, tag_id) VALUES (?1, ?2)"#,
        )
        .bind(project_id)
        .bind(tag_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}


fn string_to_hex_color(s: &str) -> String {
    let mut hash: u32 = 0;
    for byte in s.bytes() {
        hash = (byte as u32).wrapping_add(hash.wrapping_mul(31));
    }
    
    format!(
        "#{:02X}{:02X}{:02X}",
        (hash >> 16) & 0xFF,
        (hash >> 8) & 0xFF,
        hash & 0xFF
    )
}