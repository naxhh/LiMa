use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = env::var("LIMA_DATABASE_URL").unwrap_or_else(|_| "sqlite:data/state/lima.db".to_string());

    let db = lima_db::Db::connect(&database_url).await?;
    db.migrate().await?;

    tracing::info!("LIMA server started");
    Ok(())
}
