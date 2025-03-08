use std::env;

mod api;
mod db;

use api::LNMarketsAPI;
use db::DB;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lnm_api_base_url = env::var("LNM_API_BASE_URL").expect("LNM_API_BASE_URL must be set");
    let lnm_api_key = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    let lnm_api_secret = env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    let lnm_api_passphrase =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");
    let postgres_db_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("Trying to init the db...");

    DB.init(&postgres_db_url).await?;

    let price_history_entries = DB.get_all_entries().await?;

    println!("price_history_entries: {:?}", price_history_entries);

    let lnm_api = LNMarketsAPI::new(
        lnm_api_base_url,
        lnm_api_key,
        lnm_api_secret,
        lnm_api_passphrase,
    );

    let now = chrono::offset::Utc::now();

    // let hour_ago = now - chrono::Duration::hours(1);

    // println!("hour_ago {:?}", hour_ago);

    let price_history = lnm_api.futures_price_history(None, Some(now)).await?;

    for price_entry in price_history {
        match DB.add_price_entry(&price_entry).await? {
            true => println!("Price entry {:?} was added to the db", price_entry),
            false => println!("Price entry {:?} already existed in the db", price_entry),
        }
    }

    Ok(())
}
