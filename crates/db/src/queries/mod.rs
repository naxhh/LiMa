use sqlx::{Pool, Sqlite};

pub async fn ping(pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
    let (v,) = sqlx::query_as::<_, (i64,)>("SELECT 1;")
        .fetch_one(pool)
        .await?;
    
    Ok(v)
}

pub mod projects;
pub mod projects_search;
pub mod projects_create;
pub mod projects_delete;
pub mod projects_detail;
pub mod projects_update;
pub mod projects_import;
pub mod tags;
pub mod assets;