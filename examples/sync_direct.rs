//! Example demonstrating direct interaction with the sync process, without the TUI abstraction.

use std::env;

use dotenvy::dotenv;
use tokio::time::{self, Duration};

use quantoxide::{
    Database,
    sync::{SyncConfig, SyncEngine, SyncMode, SyncUpdate},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    println!("Initializing database...");

    let db = Database::new(&pg_url).await?;

    println!("Database ready. Initializing `SyncEngine`...");

    let config = SyncConfig::default();
    // How far back to fetch price history data can be configured with:
    // let config = config.with_price_history_reach(180.try_into()?); // 180 days

    let sync_engine = SyncEngine::new(config, db, domain, SyncMode::Backfill)?;

    let mut sync_rx = sync_engine.reader().update_receiver();

    tokio::spawn(async move {
        loop {
            match sync_rx.recv().await {
                Ok(sync_update) => match sync_update {
                    SyncUpdate::Status(sync_status) => {
                        println!("\n{sync_status}");
                    }
                    SyncUpdate::PriceTick(price_tick) => {
                        println!("\n{price_tick}");
                    }
                    SyncUpdate::PriceHistoryState(price_history_state) => {
                        println!("\n{price_history_state}");
                    }
                },
                Err(e) => {
                    eprint!("{:?}", e);
                    break;
                }
            }
        }
    });

    println!("Initialization OK. Starting `SyncEngine`...");

    let sync_controller = sync_engine.start();

    let final_status = sync_controller.until_stopped().await;

    // Delay for printing all `sync_rx` updates
    time::sleep(Duration::from_millis(100)).await;

    println!("Sync status: {final_status}");

    Ok(())
}
