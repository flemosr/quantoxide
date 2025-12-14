//! Example demonstrating how to run the backtest process using its TUI abstraction.

use std::env;

use dotenv::dotenv;

use quantoxide::{
    Database,
    error::Result,
    sync::PriceHistoryState,
    trade::{BacktestConfig, BacktestEngine},
    tui::{BacktestTui, TuiConfig, TuiLogger},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::raw::RawOperatorTemplate;
use util::input;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("Initializing database...");

    let db = Database::new(&pg_url).await?;

    println!("Database ready. Evaluating `PriceHistoryState`...");

    let price_history_state = PriceHistoryState::evaluate(&db).await?;

    println!("\n{price_history_state}\n");

    println!("Please provide the backtest parameters.");
    println!(
        "Note: There must be data available for the lookback period, if any, of the raw operator used.\n"
    );

    let start_time = input::prompt_date("Start date (YYYY-MM-DD): ")?;

    let start_balance =
        input::prompt_balance("Start balance (sats, default: 10000000): ", 10_000_000)?;

    let end_time = input::prompt_date("End date (YYYY-MM-DD): ")?;

    println!("\nBacktest Configuration:");
    println!("Start date: {}", start_time.format("%Y-%m-%d %H:%M %Z"));
    println!("Start balance: {}", start_balance);
    println!("End date: {}\n", end_time.format("%Y-%m-%d %H:%M %Z"));

    println!("Launching `BacktestTui`...");

    let backtest_tui = BacktestTui::launch(TuiConfig::default(), None).await?;

    backtest_tui
        .log("Initializing  `BacktestEngine`...".into())
        .await?;

    let operator = RawOperatorTemplate::new(Some(backtest_tui.clone())); // With TUI logger

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
