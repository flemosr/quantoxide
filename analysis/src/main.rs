use chrono::{DateTime, Utc};
use std::env;
use std::{thread, time};

mod api;
mod db;

use api::LNMarketsAPI;
use db::DB;

const LNM_PRICE_HISTORY_LIMIT: usize = 1000;
// Max LNM REST API rate for public endpoints is 30 requests per minute.
// Source: https://docs.lnmarkets.com/api/
const LNM_API_COOLDOWN_SEC: u64 = 3;

async fn download_price_history(
    lnm_api: &LNMarketsAPI,
    from: Option<&DateTime<Utc>>,
    mut to: Option<DateTime<Utc>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut retries: u8 = 3;
    loop {
        thread::sleep(time::Duration::from_secs(LNM_API_COOLDOWN_SEC));
        match to {
            Some(fixed_to) => println!("\nFetching price entries before {fixed_to}..."),
            None => println!("\nFetching latest price entries..."),
        }

        let price_history = match lnm_api
            .futures_price_history(None, to, Some(LNM_PRICE_HISTORY_LIMIT))
            .await
        {
            Ok(price_history) => {
                retries = 3;
                price_history
            }
            Err(e) => {
                println!("\nError fetching price history {:?}", e);
                if retries == 1 {
                    return Err(e);
                }
                retries -= 1;
                println!("\nRemaining retries: {retries}");
                continue;
            }
        };

        if price_history.len() < LNM_PRICE_HISTORY_LIMIT {
            panic!(
                "Received only {} price entries with limit {LNM_PRICE_HISTORY_LIMIT}.",
                price_history.len()
            );
        }

        if let Some(fixed_to) = to {
            let first_entry_time = *price_history.first().expect("not empty").time();
            if first_entry_time != fixed_to {
                panic!("Tried to add price entries without overlap.");
            }
        }

        for price_entry in price_history.iter() {
            if let Some(fixed_from) = from {
                if *price_entry.time() <= *fixed_from {
                    // Reached `from` limit
                    println!("\nReached `from` limit {fixed_from}.");
                    return Ok(());
                }
            }

            match DB.add_price_entry(&price_entry).await? {
                true => println!("Price entry {:?} was added to the DB.", price_entry),
                false => println!("Price entry {:?} already existed in the DB.", price_entry),
            }
        }

        to = Some(*price_history.last().expect("not empty").time());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lnm_api_base_url = env::var("LNM_API_BASE_URL").expect("LNM_API_BASE_URL must be set");
    let lnm_api_key = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    let lnm_api_secret = env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    let lnm_api_passphrase =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");
    let postgres_db_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("Trying to init the DB...");

    DB.init(&postgres_db_url).await?;

    println!("DB is ready.");

    let lnm_api = LNMarketsAPI::new(
        lnm_api_base_url,
        lnm_api_key,
        lnm_api_secret,
        lnm_api_passphrase,
    );

    if let Some(latest_price_entry) = DB.get_latest_price_entry().await? {
        println!(
            "\nDownloading the latest price entries until reaching the latest ({}) in the DB...",
            latest_price_entry.time
        );

        download_price_history(&lnm_api, Some(&latest_price_entry.time), None).await?;
    }

    if let Some(earliest_price_entry) = DB.get_earliest_price_entry().await? {
        println!(
            "\nDownloading price entries from the earliest ({}) in the DB backwards...",
            earliest_price_entry.time
        );
        download_price_history(&lnm_api, None, Some(earliest_price_entry.time)).await?;
    } else {
        println!("\nNo price entries in the DB. Downloading from latest to earliest...");
        download_price_history(&lnm_api, None, None).await?;
    }

    Ok(())
}
