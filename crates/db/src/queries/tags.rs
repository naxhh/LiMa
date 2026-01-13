use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

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

        let id = Uuid::new_v4().to_string();
        let color = string_to_hex_color(name);

        sqlx::query(
            r#"INSERT INTO tags (id, name, color, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)"#,
        )
        .bind(&id)
        .bind(&name)
        .bind(&color)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        ids.push(id);

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