use analysis::{
    api::{self, rest::RestContext, websocket},
    db::DbContext,
    error::Result,
};

mod env;
mod sync;

use env::{LNM_API_DOMAIN, POSTGRES_DB_URL};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    let db = DbContext::new(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Trying to init `api`...");

    let api_domain = LNM_API_DOMAIN.to_string();

    let rest = RestContext::new(api_domain.clone());
    let ws = websocket::new(api_domain).await?;

    println!("`api` is ready. Starting `sync`...");

    sync::start(&rest, &ws, &db).await
}
