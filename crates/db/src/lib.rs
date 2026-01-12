use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use sqlx::migrate::MigrateError;
use std::time::Duration;

pub struct Db {
    pool: Pool<Sqlite>,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(10))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys = ON;").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA journal_mode = WAL;").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA synchronous = NORMAL;").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA busy_timeout = 5000;").execute(&mut *conn).await?;
                    Ok(())
                })
            })
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), MigrateError> {
        sqlx::migrate!("../../migrations").run(&self.pool).await
    }
}

pub mod queries;