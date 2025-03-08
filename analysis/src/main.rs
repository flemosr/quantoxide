use std::env;
use std::{thread, time};

mod api;
mod db;

use api::LNMarketsAPI;
use db::DB;

const LNM_PRICE_HISTORY_LIMIT: usize = 1000;

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

    let lnm_api = LNMarketsAPI::new(
        lnm_api_base_url,
        lnm_api_key,
        lnm_api_secret,
        lnm_api_passphrase,
    );

    println!(
        "Syncing the most recent price entries until reaching the latest price entry in the DB...\n"
    );

    if let Some(latest_price_entry) = DB.get_latest_price_entry().await? {
        let mut first_fetch = true;
        let mut get_history_to = chrono::offset::Utc::now();

        'outer: loop {
            thread::sleep(time::Duration::from_secs(5));
            println!("Getting futures price history to {get_history_to}...");

            let price_history = lnm_api
                .futures_price_history(None, Some(get_history_to), Some(LNM_PRICE_HISTORY_LIMIT))
                .await?;

            if price_history.len() < LNM_PRICE_HISTORY_LIMIT {
                panic!(
                    "Received only {} price entries with limit {LNM_PRICE_HISTORY_LIMIT}",
                    price_history.len()
                );
            }

            if first_fetch == false && *price_history.first().unwrap().time() != get_history_to {
                panic!("Tried to add entries without overlap");
            }

            first_fetch = false;

            for price_entry in price_history {
                match DB.add_price_entry(&price_entry).await? {
                    true => {
                        println!("Price entry {:?} was added to the db", price_entry);
                        get_history_to = *price_entry.time();
                    }
                    false => {
                        println!("Price entry {:?} already existed in the db", price_entry);
                        if *price_entry.time() < latest_price_entry.time {
                            // All price entries from this point onwards will be present in the DB.
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    let mut empty_db = true;
    let mut get_history_to =
        if let Some(earliest_price_entry) = DB.get_earliest_price_entry().await? {
            empty_db = false;
            earliest_price_entry.time
        } else {
            chrono::offset::Utc::now()
        };

    println!("\nSyncing older price entries, from the earliest price entry in the DB...\n");

    loop {
        thread::sleep(time::Duration::from_secs(5));
        println!("Getting futures price history to {get_history_to}...");

        let price_history = lnm_api
            .futures_price_history(None, Some(get_history_to), Some(LNM_PRICE_HISTORY_LIMIT))
            .await?;

        if price_history.len() < LNM_PRICE_HISTORY_LIMIT {
            panic!(
                "Received only {} price entries with limit {LNM_PRICE_HISTORY_LIMIT}",
                price_history.len()
            );
        }

        if empty_db == false && *price_history.first().unwrap().time() != get_history_to {
            panic!("Tried to add entries without overlap");
        }

        for price_entry in price_history {
            match DB.add_price_entry(&price_entry).await? {
                true => {
                    println!("Price entry {:?} was added to the db", price_entry);
                    get_history_to = *price_entry.time();
                }
                false => {
                    println!("Price entry {:?} already existed in the db", price_entry);
                }
            }
            empty_db = false;
        }
    }
}
