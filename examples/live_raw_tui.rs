//! Example demonstrating how to run the live trading process with a raw operator, using its TUI
//! abstraction.

use std::env;

use dotenvy::dotenv;

use quantoxide::{
    Database,
    error::Result,
    trade::{LiveTradeConfig, LiveTradeEngine},
    tui::{LiveTui, TuiConfig},
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

    println!("Launching `LiveTui`...");

    let live_tui = LiveTui::launch(TuiConfig::default(), None).await?;

    // Direct `stdout`/`stderr` outputs will corrupt the TUI. Use `live_tui.log()` instead
    live_tui.log("Initializing database...".into()).await?;

    let db = Database::new(&pg_url).await?;

    live_tui
        .log("Database ready. Initializing `LiveTradeEngine`...".into())
        .await?;

    // Pass TUI logger to Trade Operator
    let operator = RawOperatorTemplate::with_logger(live_tui.as_logger());

    let live_engine = LiveTradeEngine::with_raw_operator(
        LiveTradeConfig::default(),
        db,
        domain,
        key,
        secret,
        passphrase,
        operator,
    )?;

    live_tui
        .log("Initialization OK. Coupling `LiveTradeEngine`...".into())
        .await?;

    live_tui.couple(live_engine).await?;

    let final_status = live_tui.until_stopped().await;
    println!("`LiveTui` status: {final_status}");

    Ok(())
}
