mod api;
mod db;
mod env;
mod sync;

use crate::db::DB;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init the DB...");

    DB.init().await?;

    println!("DB is ready.");

    sync::start().await
}
