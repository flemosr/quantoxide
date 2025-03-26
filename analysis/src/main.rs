mod api;
mod db;
mod env;
mod error;
mod sync;

use error::Result;

use crate::env::{LNM_API_DOMAIN, POSTGRES_DB_URL};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    db::init(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Trying to init `api`...");

    api::init(LNM_API_DOMAIN.to_string())?;

    println!("`api` is ready. Starting `sync`...");

    sync::start().await
}
