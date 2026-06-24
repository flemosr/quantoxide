//! Example demonstrating a simulated cross-margin carry trade in a backtest.
//!
//! The shared raw operator moves enough starting balance into cross margin to target a short
//! liquidation price 20% above the current market price, opens a configurable short hedge as a
//! percentage of account net value in USD, and rebalances whenever the hedge drifts more than 1%
//! away from the hedge target. Positive funding rates are expected to pay shorts; in the simulator,
//! cross funding receipts are reflected in cross margin and therefore in the account net value used
//! as the hedge target.

use std::env;

use dotenvy::dotenv;
use tokio::time::{self, Duration};

use quantoxide::{
    Database,
    error::Result,
    models::{PercentageCapped, SATS_PER_BTC},
    sync::PriceHistoryState,
    trade::{BacktestConfig, BacktestEngine, BacktestStatus, BacktestUpdate, TradingState},
};

#[path = "operators/mod.rs"]
mod operators;
#[path = "util/mod.rs"]
mod util;

use operators::cross_carry::{CrossCarryOperator, CrossCarryOperatorConfig};
use util::input;

const DEFAULT_START_BALANCE_SATS: u64 = 10_000_000;
const DEFAULT_HEDGE_PERC: f64 = 100.0;

fn print_final_summary(state: &TradingState) {
    let cross_position = state.cross_position();
    let account_net_value_usd =
        state.total_net_value() as f64 * state.market_price().as_f64() / SATS_PER_BTC;
    let hedged_value_usd = -cross_position.quantity() as f64;
    let hedge_drift_usd = account_net_value_usd - hedged_value_usd;
    let hedge_drift_percent = if account_net_value_usd.abs() <= f64::EPSILON {
        0.0
    } else {
        hedge_drift_usd / account_net_value_usd * 100.0
    };

    println!(
        "\nFinal: time={}, net={} sats (${account_net_value_usd:.2}), hedge=${hedged_value_usd:.2}, drift={hedge_drift_usd:+.2} ({hedge_drift_percent:+.2}%), cross_qty={} USD, cross_margin={} sats",
        state.last_tick_time(),
        state.total_net_value(),
        cross_position.quantity(),
        cross_position.margin()
    );
}

/// Prints usage information and exits.
fn print_usage() {
    eprintln!(
        "Usage: cargo run --example backtest_cross_carry -- --start <DATE> --end <DATE> [OPTIONS]"
    );
    eprintln!();
    eprintln!("Required:");
    eprintln!("  --start <DATE>       Start date in YYYY-MM-DD format");
    eprintln!("  --end <DATE>         End date in YYYY-MM-DD format");
    eprintln!();
    eprintln!("Options:");
    eprintln!(
        "  --balance <SATS>     Starting balance in sats (default: {DEFAULT_START_BALANCE_SATS})"
    );
    eprintln!(
        "  --hedge-perc <PCT>   Target hedge percentage of account NAV (default: {DEFAULT_HEDGE_PERC})"
    );
    eprintln!();
    eprintln!("Example:");
    eprintln!(
        "  cargo run --example backtest_cross_carry -- --start 2025-09-01 --end 2025-09-02 --balance {DEFAULT_START_BALANCE_SATS} --hedge-perc {DEFAULT_HEDGE_PERC}"
    );
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

    let args = input::parse_args();

    let Some(start_str) = args.get("start") else {
        print_usage();
        return Err("Missing required argument: --start".into());
    };

    let Some(end_str) = args.get("end") else {
        print_usage();
        return Err("Missing required argument: --end".into());
    };

    let start_time = input::parse_date(start_str).map_err(|e| {
        eprintln!("Error parsing --start: {}", e);
        print_usage();
        e
    })?;

    let end_time = input::parse_date(end_str).map_err(|e| {
        eprintln!("Error parsing --end: {}", e);
        print_usage();
        e
    })?;

    let start_balance = match args.get("balance") {
        Some(v) => v.parse::<u64>().map_err(|e| {
            eprintln!("Error parsing --balance: {}", e);
            print_usage();
            e
        })?,
        None => DEFAULT_START_BALANCE_SATS,
    };

    if start_balance == 0 {
        print_usage();
        return Err("--balance must be greater than zero".into());
    }

    let hedge_perc = match args.get("hedge-perc") {
        Some(v) => input::parse_percentage_capped(v).map_err(|e| {
            eprintln!("Error parsing --hedge-perc: {}", e);
            print_usage();
            e
        })?,
        None => PercentageCapped::bounded(DEFAULT_HEDGE_PERC),
    };

    println!("\nBacktest Cross-Margin Carry Trade Configuration:");
    println!("Start date: {}", start_time.format("%Y-%m-%d %H:%M %Z"));
    println!("Start balance: {} sats", start_balance);
    println!("Hedge percentage: {:.2}%", hedge_perc.as_f64());
    println!("End date: {}\n", end_time.format("%Y-%m-%d %H:%M %Z"));

    println!("Initializing `BacktestEngine`...");

    let operator = CrossCarryOperator::new(CrossCarryOperatorConfig::default(), hedge_perc);

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
                    BacktestUpdate::Status(backtest_status) => match backtest_status {
                        BacktestStatus::Finished => {
                            if let Some(state) = last_trading_state {
                                print_final_summary(&state);
                            }
                            return;
                        }
                        BacktestStatus::Failed(error) => {
                            eprintln!("\nBacktest failed: {error}");
                            return;
                        }
                        BacktestStatus::Aborted => {
                            println!("\nBacktest aborted.");
                            return;
                        }
                        BacktestStatus::NotInitiated
                        | BacktestStatus::Starting
                        | BacktestStatus::Running => {}
                    },
                    BacktestUpdate::TradingState(trading_state) => {
                        last_trading_state = Some(trading_state);
                    }
                },
                Err(e) => {
                    eprintln!("{e:?}");
                    return;
                }
            }
        }
    });

    println!("Initialization OK. Starting `BacktestEngine`...");

    let backtest_controller = backtest_engine.start();

    let final_status = backtest_controller.until_stopped().await;

    // Delay for printing all `backtest_rx` updates.
    time::sleep(Duration::from_millis(100)).await;

    println!("\nBacktest status: {final_status}");

    Ok(())
}
