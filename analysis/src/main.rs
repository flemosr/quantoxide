mod api;
mod db;
mod env;
mod sync;

use crate::env::POSTGRES_DB_URL;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init the DB...");

    db::init(&POSTGRES_DB_URL).await?;

    println!("DB is ready.");

    sync::start().await
}
