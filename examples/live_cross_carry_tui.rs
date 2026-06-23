//! Example demonstrating how to run the cross-margin carry trade with the live trading TUI.
//!
//! **Warning:** this example uses live LN Markets credentials and can place real cross-margin
//! market orders and transfer real account balance between isolated/free balance and cross margin.
//! Test the shared operator with `backtest_cross_carry_tui` before adapting this example for a live
//! account.

use std::env;

use dotenvy::dotenv;
use lazy_static::lazy_static;

use quantoxide::{
    Database,
    error::Result,
    models::{CrossLeverage, Percentage, PercentageCapped},
    trade::{LiveTradeConfig, LiveTradeEngine},
    tui::{LiveTui, TuiConfig},
};

#[path = "operators/mod.rs"]
mod operators;

use operators::cross_carry::CrossCarryOperator;

const LIVE_FUNDS_WARNING: &str = "WARNING: live_cross_carry_tui can place real cross-margin market \
orders and transfer real account balance between isolated/free balance and cross margin.";

lazy_static! {
    static ref CROSS_LEVERAGE: CrossLeverage = CrossLeverage::bounded(10);
    static ref REBALANCE_THRESHOLD_PERCENT: PercentageCapped = PercentageCapped::bounded(1.0);
    static ref TARGET_LIQUIDATION_BUFFER: Percentage = Percentage::bounded(20.0);
    static ref LIQ_TOLERANCE: PercentageCapped = PercentageCapped::bounded(5.0);
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");
    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY must be set");
    let secret = env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE").expect("LNM_API_V3_PASSPHRASE must be set");

    println!("{LIVE_FUNDS_WARNING}");
    println!("Stop now if you have not reviewed the operator and your LN Markets account state.\n");

    println!("Live Cross-Margin Carry Trade TUI Configuration:");
    println!(
        "Cross deposit: dynamic, targeting short liquidation {:.2}% above market",
        TARGET_LIQUIDATION_BUFFER.as_f64()
    );
    println!("Cross leverage: {}x", CROSS_LEVERAGE.as_u64());
    println!(
        "Rebalance threshold: {:.2}%",
        REBALANCE_THRESHOLD_PERCENT.as_f64()
    );
    println!("Liquidation tolerance: {:.2}%\n", LIQ_TOLERANCE.as_f64());

    println!("Launching `LiveTui`...");

    let live_tui = LiveTui::launch(TuiConfig::default(), None).await?;

    // Direct `stdout`/`stderr` outputs will corrupt the TUI. Use `live_tui.log()` instead.
    live_tui.log(LIVE_FUNDS_WARNING.into()).await?;
    live_tui.log("Initializing database...".into()).await?;

    let db = Database::new(&pg_url).await?;

    live_tui
        .log("Database ready. Initializing `LiveTradeEngine`...".into())
        .await?;

    let live_config = LiveTradeConfig::default();
    let operator = CrossCarryOperator::with_logger(
        *CROSS_LEVERAGE,
        *REBALANCE_THRESHOLD_PERCENT,
        *TARGET_LIQUIDATION_BUFFER,
        *LIQ_TOLERANCE,
        live_config.trade_estimated_fee(),
        live_tui.as_logger(),
    );

    let live_engine = LiveTradeEngine::with_raw_operator(
        live_config,
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
