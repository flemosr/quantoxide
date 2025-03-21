mod api;
mod db;
mod env;
mod sync;

use crate::db::DB;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Trying to init the DB...");

    DB.init().await?;

    println!("DB is ready.");

    sync::start().await
}
