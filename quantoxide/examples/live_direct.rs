//! Example demonstrating direct interaction with the live trading process, without the TUI
//! abstraction.

use std::env;

use dotenv::dotenv;
use tokio::time::{self, Duration};

use quantoxide::{
    Database,
    error::Result,
    trade::{LiveTradeConfig, LiveTradeEngine, LiveTradeUpdate},
};

#[path = "operators/mod.rs"]
mod operators;

use operators::raw::RawOperatorTemplate;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");
    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY must be set");
    let secret = env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE").expect("LNM_API_V3_PASSPHRASE must be set");

    println!("Initializing database...");

    let db = Database::new(&pg_url).await?;

    println!("Database ready. Initializing `LiveTradeEngine`...");

    let operator = RawOperatorTemplate::new(None); // No TUI logger provided

    let live_engine = LiveTradeEngine::with_raw_operator(
        LiveTradeConfig::default(),
        db,
        domain,
        key,
        secret,
        passphrase,
        operator,
    )?;

    let mut live_rx = live_engine.update_receiver();

    tokio::spawn(async move {
        loop {
            match live_rx.recv().await {
                Ok(live_update) => match live_update {
                    LiveTradeUpdate::Status(live_status) => {
                        println!("{live_status}");
                    }
                    LiveTradeUpdate::Signal(signal) => {
                        println!("{signal}");
                    }
                    LiveTradeUpdate::ClosedTrade(closed_trade) => {
                        println!("{closed_trade}");
                    }
                    LiveTradeUpdate::Order(order) => {
                        println!("{order}");
                    }
                    LiveTradeUpdate::TradingState(trading_state) => {
                        println!("{trading_state}");
                    }
                },
                Err(e) => {
                    eprint!("{:?}", e);
                    break;
                }
            }
        }
    });

    println!("Initialization OK. Starting `LiveTradeEngine`...");

    let live_controller = live_engine.start().await?;

    let final_status = live_controller.until_stopped().await;

    // Delay for printing all `live_rx` updates
    time::sleep(Duration::from_millis(100)).await;

    println!("Live trade status: {final_status}");

    Ok(())
}
