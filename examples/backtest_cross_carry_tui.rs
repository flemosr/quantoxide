//! Example demonstrating how to run the simulated cross-margin carry trade with the backtest TUI.
//!
//! The shared raw operator moves the full starting isolated balance into the cross-margin account,
//! opens a short hedge equal to the account net value in USD, and rebalances whenever the hedge
//! drifts more than 1% away from the current USD value of the account.

use std::env;

use dotenvy::dotenv;
use lazy_static::lazy_static;

use quantoxide::{
    Database,
    error::Result,
    models::{CrossLeverage, PercentageCapped},
    sync::PriceHistoryState,
    trade::{BacktestConfig, BacktestEngine},
    tui::{BacktestTui, TuiConfig},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::cross_carry::CrossCarryOperator;
use util::input;

const DEFAULT_START_BALANCE_SATS: u64 = 10_000_000;

lazy_static! {
    static ref CROSS_LEVERAGE: CrossLeverage = CrossLeverage::bounded(10);
    static ref REBALANCE_THRESHOLD_PERCENT: PercentageCapped = PercentageCapped::bounded(1.0);
}

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
    println!("Cross deposit: full isolated balance");
    println!("Cross leverage: {}x", CROSS_LEVERAGE.as_u64());
    println!(
        "Rebalance threshold: {:.2}%",
        REBALANCE_THRESHOLD_PERCENT.as_f64()
    );
    println!("End date: {}\n", end_time.format("%Y-%m-%d %H:%M %Z"));

    println!("Launching `BacktestTui`...");

    let backtest_tui = BacktestTui::launch(TuiConfig::default(), None).await?;

    // Direct `stdout`/`stderr` outputs will corrupt the TUI. Use `backtest_tui.log()` instead.
    backtest_tui
        .log("Initializing `BacktestEngine`...".into())
        .await?;

    let operator = CrossCarryOperator::with_logger(
        *CROSS_LEVERAGE,
        *REBALANCE_THRESHOLD_PERCENT,
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
