use sqlx::{Pool, Sqlite};

pub async fn ping(pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
    let (v,) = sqlx::query_as::<_, (i64,)>("SELECT 1;")
        .fetch_one(pool)
        .await?;
    
    Ok(v)
}

pub mod projects;
