//! Example demonstrating direct interaction with the backtest process, without the TUI abstraction.

use std::env;

use dotenv::dotenv;
use serde_json::json;
use tokio::time::{self, Duration};

use quantoxide::{
    Database,
    error::Result,
    sync::PriceHistoryState,
    trade::{BacktestConfig, BacktestEngine, BacktestStatus, BacktestUpdate, TradingState},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::raw::RawOperatorTemplate;
use util::input;

/// Prints usage information and exits.
fn print_usage() {
    eprintln!(
        "Usage: cargo run --example backtest_direct -- <start_date> <end_date> [start_balance]"
    );
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  start_date      Start date in YYYY-MM-DD format");
    eprintln!("  end_date        End date in YYYY-MM-DD format");
    eprintln!("  start_balance   Optional starting balance in sats (default: 10000000)");
    eprintln!();
    eprintln!("Example:");
    eprintln!("  cargo run --example backtest_direct -- 2025-09-01 2025-12-01 10000000");
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("Initializing database...");

    let db = Database::new(&pg_url).await?;

    println!("Database ready. Evaluating `PriceHistoryState`...");

    let price_history_state = PriceHistoryState::evaluate(&db).await?;

    println!("\n{price_history_state}");

    if price_history_state.bound_end().is_none() {
        println!(
            "\nSome price history must be available in the local database to run the backtest."
        );
        println!("Run a synchronization example first to fetch historical data.");

        return Ok(());
    }

    // Parse CLI arguments (skip the first argument which is the program name)
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() < 2 {
        print_usage();
        return Err("Insufficient arguments: start_date and end_date are required".into());
    }

    let start_time = input::parse_date(&args[0]).map_err(|e| {
        eprintln!("Error parsing start_date: {}", e);
        print_usage();
        e
    })?;

    let end_time = input::parse_date(&args[1]).map_err(|e| {
        eprintln!("Error parsing end_date: {}", e);
        print_usage();
        e
    })?;

    let start_balance = if args.len() >= 3 {
        args[2].parse::<u64>().map_err(|e| {
            eprintln!("Error parsing start_balance: {}", e);
            print_usage();
            e
        })?
    } else {
        10_000_000
    };

    println!("\nBacktest Configuration:");
    println!("Start date: {}", start_time.format("%Y-%m-%d %H:%M %Z"));
    println!("Start balance: {}", start_balance);
    println!("End date: {}\n", end_time.format("%Y-%m-%d %H:%M %Z"));

    println!("Initializing `BacktestEngine`...");

    let operator = RawOperatorTemplate::new();

    let backtest_engine = BacktestEngine::with_raw_operator(
        BacktestConfig::default(),
        db,
        operator,
        start_time,
        start_balance,
        end_time,
    )
    .await?;

    let mut backtest_rx = backtest_engine.receiver();

    tokio::spawn(async move {
        let mut last_trading_state: Option<TradingState> = None;
        loop {
            match backtest_rx.recv().await {
                Ok(backtest_update) => match backtest_update {
                    BacktestUpdate::Status(backtest_status) => {
                        if matches!(backtest_status, BacktestStatus::Finished) {
                            if let Some(state) = last_trading_state {
                                println!("\nBacktest finished.\n\nFinal {state}");

                                let final_net_value = state.total_net_value();
                                let pnl = final_net_value as i64 - start_balance as i64;
                                let pnl_pct = (pnl as f64 / start_balance as f64) * 100.0;

                                let summary = json!({
                                    "backtest_summary": {
                                        "start_balance_sats": start_balance,
                                        "final_net_value_sats": final_net_value,
                                        "pnl_sats": pnl,
                                        "pnl_percent": format!("{:.2}", pnl_pct),
                                        "profitable": pnl > 0,
                                    }
                                });
                                let summary = serde_json::to_string_pretty(&summary).unwrap();
                                println!("\n{summary}");
                            }
                            return;
                        }
                    }
                    BacktestUpdate::TradingState(trading_state) => {
                        last_trading_state = Some(trading_state);
                    }
                },
                Err(e) => {
                    eprint!("{:?}", e);
                    return;
                }
            }
        }
    });

    println!("Initialization OK. Starting `BacktestEngine`...");

    let backtest_controller = backtest_engine.start();

    let final_status = backtest_controller.until_stopped().await;

    // Delay for printing all `backtest_rx` updates
    time::sleep(Duration::from_millis(100)).await;

    println!("\nBacktest status: {final_status}");

    Ok(())
}
