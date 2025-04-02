use analysis::{api::ApiContext, db::DbContext, error::Result};

mod env;
mod sync;

use env::{LNM_API_DOMAIN, POSTGRES_DB_URL};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    let db = DbContext::new(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Trying to init `api`...");

    let api = ApiContext::new(LNM_API_DOMAIN.to_string());

    println!("`api` is ready. Starting `sync`...");

    sync::start(&api, &db).await
}
