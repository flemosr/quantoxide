//! Example demonstrating how to run the cross-margin carry trade with the live trading TUI.
//!
//! **Warning:** this example uses live LN Markets credentials and can place real cross-margin
//! market orders and transfer real account balance between isolated/free balance and cross margin.
//! Test the shared operator with `backtest_cross_carry_tui` before adapting this example for a live
//! account.

use std::env;

use dotenvy::dotenv;

use quantoxide::{
    Database,
    error::Result,
    models::PercentageCapped,
    trade::{LiveTradeConfig, LiveTradeEngine},
    tui::{LiveTui, TuiConfig},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::cross_carry::{CrossCarryOperator, CrossCarryOperatorConfig};
use util::input;

const DEFAULT_HEDGE_PERC: f64 = 100.0;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");
    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY must be set");
    let secret = env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE").expect("LNM_API_V3_PASSPHRASE must be set");

    println!("Stop now if you have not reviewed the operator and your LN Markets account state.\n");

    let hedge_perc = input::prompt_percentage_capped(
        &format!("Hedge percentage (default: {DEFAULT_HEDGE_PERC}): "),
        PercentageCapped::bounded(DEFAULT_HEDGE_PERC),
    )?;

    println!("\nLive Cross-Margin Carry Trade TUI Configuration:");
    println!("Hedge percentage: {:.2}%\n", hedge_perc.as_f64());

    println!("Launching `LiveTui`...");

    let live_tui = LiveTui::launch(TuiConfig::default(), None).await?;

    // Direct `stdout`/`stderr` outputs will corrupt the TUI. Use `live_tui.log()` instead.
    live_tui.log("Initializing database...".into()).await?;

    let db = Database::new(&pg_url).await?;

    live_tui
        .log("Database ready. Initializing `LiveTradeEngine`...".into())
        .await?;

    let operator = CrossCarryOperator::with_logger(
        CrossCarryOperatorConfig::default(),
        hedge_perc,
        live_tui.as_logger(),
    );

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
