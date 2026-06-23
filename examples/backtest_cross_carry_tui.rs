//! Example demonstrating how to run the simulated cross-margin carry trade with the backtest TUI.
//!
//! The shared raw operator moves enough starting balance into cross margin to target a short
//! liquidation price 20% above the current market price, opens a short hedge equal to the account
//! net value in USD, and rebalances whenever the hedge drifts more than 1% away from the current USD
//! value of the account.

use std::env;

use dotenvy::dotenv;

use quantoxide::{
    Database,
    error::Result,
    sync::PriceHistoryState,
    trade::{BacktestConfig, BacktestEngine},
    tui::{BacktestTui, TuiConfig},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::cross_carry::{CrossCarryOperator, CrossCarryOperatorConfig};
use util::input;

const DEFAULT_START_BALANCE_SATS: u64 = 10_000_000;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("Initializing database...");

    let db = Database::new(&pg_url).await?;

    println!("Database ready. Evaluating `PriceHistoryState`...");

    let price_history_state = PriceHistoryState::evaluate(&db).await?;

    println!("\n{price_history_state}\n");

    if price_history_state.bound_end().is_none() {
        println!("Some price history must be available in the local database to run the backtest.");
        println!("Run a synchronization example, in backfill mode, to fetch historical data.");

        return Ok(());
    }

    println!("Please provide the cross-margin carry trade backtest parameters.\n");

    let start_time = input::prompt_date("Start date (YYYY-MM-DD): ")?;

    let start_balance = input::prompt_balance(
        &format!("Start balance (sats, default: {DEFAULT_START_BALANCE_SATS}): "),
        DEFAULT_START_BALANCE_SATS,
    )?;

    let end_time = input::prompt_date("End date (YYYY-MM-DD): ")?;

    if start_balance == 0 {
        return Err("start balance must be greater than zero".into());
    }

    println!("\nBacktest Cross-Margin Carry Trade TUI Configuration:");
    println!("Start date: {}", start_time.format("%Y-%m-%d %H:%M %Z"));
    println!("Start balance: {} sats", start_balance);
    println!("End date: {}\n", end_time.format("%Y-%m-%d %H:%M %Z"));

    println!("Launching `BacktestTui`...");

    let backtest_tui = BacktestTui::launch(TuiConfig::default(), None).await?;

    // Direct `stdout`/`stderr` outputs will corrupt the TUI. Use `backtest_tui.log()` instead.
    backtest_tui
        .log("Initializing `BacktestEngine`...".into())
        .await?;

    let operator = CrossCarryOperator::with_logger(
        CrossCarryOperatorConfig::default(),
        backtest_tui.as_logger(),
    );

    let backtest_engine = BacktestEngine::with_raw_operator(
        BacktestConfig::default(),
        db,
        operator,
        start_time,
        start_balance,
        end_time,
    )
    .await?;

    backtest_tui
        .log("Initialization OK. Coupling `BacktestEngine`...".into())
        .await?;

    backtest_tui.couple(backtest_engine).await?;

    let final_status = backtest_tui.until_stopped().await;
    println!("`BacktestTui` status: {final_status}");

    Ok(())
}
