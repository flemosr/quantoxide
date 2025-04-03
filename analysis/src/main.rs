use analysis::{api::ApiContext, db::DbContext, error::Result, sync::Sync};

mod env;

use env::{
    LNM_API_COOLDOWN_SEC, LNM_API_DOMAIN, LNM_API_ERROR_COOLDOWN_SEC, LNM_API_ERROR_MAX_TRIALS,
    LNM_PRICE_HISTORY_LIMIT, POSTGRES_DB_URL, SYNC_HISTORY_REACH_WEEKS,
};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    let db = DbContext::new(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Init `api`...");

    let api = ApiContext::new(LNM_API_DOMAIN.to_string());

    println!("`api` is ready. Starting `sync`...");

    let sync = Sync::new(
        *LNM_API_COOLDOWN_SEC,
        *LNM_API_ERROR_COOLDOWN_SEC,
        *LNM_API_ERROR_MAX_TRIALS,
        *LNM_PRICE_HISTORY_LIMIT,
        *SYNC_HISTORY_REACH_WEEKS,
    );

    sync.start(&api, &db).await
}
