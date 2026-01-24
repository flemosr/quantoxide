//! Example demonstrating direct interaction with the backtest process, without the TUI abstraction.

use std::env;

use dotenv::dotenv;
use serde_json::json;
use tokio::time::{self, Duration};

use quantoxide::{
    Database,
    error::Result,
    models::SATS_PER_BTC,
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
        "Usage: cargo run --example backtest_direct -- --start <DATE> --end <DATE> [OPTIONS]"
    );
    eprintln!();
    eprintln!("Required:");
    eprintln!("  --start <DATE>       Start date in YYYY-MM-DD format");
    eprintln!("  --end <DATE>         End date in YYYY-MM-DD format");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --balance <SATS>     Starting balance in sats (default: 10000000)");
    eprintln!("  --rfr-sats <RATE>    Annual risk-free rate for sats as decimal (default: 0.0)");
    eprintln!("  --rfr-usd <RATE>     Annual risk-free rate for USD as decimal (default: 0.0)");
    eprintln!();
    eprintln!("Example:");
    eprintln!(
        "  cargo run --example backtest_direct -- --start 2025-09-01 --end 2025-12-01 --balance 10000000 --rfr-sats 0.0 --rfr-usd 0.05"
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
        None => 10_000_000,
    };

    let rfr_sats = match args.get("rfr-sats") {
        Some(v) => v.parse::<f64>().map_err(|e| {
            eprintln!("Error parsing --rfr-sats: {}", e);
            print_usage();
            e
        })?,
        None => 0.0,
    };

    let rfr_usd = match args.get("rfr-usd") {
        Some(v) => v.parse::<f64>().map_err(|e| {
            eprintln!("Error parsing --rfr-usd: {}", e);
            print_usage();
            e
        })?,
        None => 0.0,
    };

    println!("\nBacktest Configuration:");
    println!("Start date: {}", start_time.format("%Y-%m-%d %H:%M %Z"));
    println!("Start balance: {}", start_balance);
    println!("Risk-free rate (sats): {:.2}%", rfr_sats * 100.0);
    println!("Risk-free rate (USD): {:.2}%", rfr_usd * 100.0);
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
        let mut start_market_price: Option<f64> = None;
        let mut daily_net_values_sats: Vec<f64> = Vec::new();
        let mut daily_net_values_usd: Vec<f64> = Vec::new();

        loop {
            match backtest_rx.recv().await {
                Ok(backtest_update) => match backtest_update {
                    BacktestUpdate::Status(backtest_status) => {
                        if matches!(backtest_status, BacktestStatus::Finished) {
                            if let Some(state) = last_trading_state {
                                println!("\nFinal {state}");

                                let trade_count =
                                    state.closed_history().len() + state.running_map().len();

                                let final_net_value_sats = state.total_net_value();
                                let final_market_price = state.market_price().as_f64();

                                let start_price = start_market_price.unwrap_or(final_market_price);
                                let start_balance_usd =
                                    start_balance as f64 * start_price / SATS_PER_BTC as f64;
                                let final_net_value_usd = final_net_value_sats as f64
                                    * final_market_price
                                    / SATS_PER_BTC as f64;

                                let pnl_sats = final_net_value_sats as i64 - start_balance as i64;
                                let pnl_sats_pct = (pnl_sats as f64 / start_balance as f64) * 100.0;
                                let pnl_usd = final_net_value_usd - start_balance_usd;
                                let pnl_usd_pct = (pnl_usd / start_balance_usd) * 100.0;

                                let sharpe_sats =
                                    calculate_sharpe_ratio(&daily_net_values_sats, rfr_sats);
                                let sharpe_usd =
                                    calculate_sharpe_ratio(&daily_net_values_usd, rfr_usd);

                                let format_sharpe = |s: Option<f64>| match s {
                                    Some(v) => format!("{:.4}", v),
                                    None => "N/A".to_string(),
                                };

                                let summary = json!({
                                    "backtest_summary": {
                                        "start_market_price": format!("{:.2}", start_price),
                                        "final_market_price": format!("{:.2}", final_market_price),
                                        "trade_count": trade_count,
                                        "start_balance_sats": start_balance,
                                        "start_balance_usd": format!("{:.2}", start_balance_usd),
                                        "final_net_value_sats": final_net_value_sats,
                                        "final_net_value_usd": format!("{:.2}", final_net_value_usd),
                                        "pnl_sats": pnl_sats,
                                        "pnl_sats_percent": format!("{:.2}", pnl_sats_pct),
                                        "pnl_usd": format!("{:.2}", pnl_usd),
                                        "pnl_usd_percent": format!("{:.2}", pnl_usd_pct),
                                        "daily_data_points": daily_net_values_sats.len(),
                                        "sharpe_sats": format_sharpe(sharpe_sats),
                                        "sharpe_usd": format_sharpe(sharpe_usd),
                                    }
                                });

                                let summary = serde_json::to_string_pretty(&summary).unwrap();
                                println!("\n{summary}");
                            }
                            return;
                        }
                    }
                    BacktestUpdate::TradingState(trading_state) => {
                        // Backtest updates correspond to midnight (UTC) of each day of the period

                        if start_market_price.is_none() {
                            start_market_price = Some(trading_state.market_price().as_f64());
                        }

                        let net_value_sats = trading_state.total_net_value() as f64;
                        let market_price = trading_state.market_price().as_f64();
                        let net_value_usd = net_value_sats * market_price / SATS_PER_BTC as f64;

                        daily_net_values_sats.push(net_value_sats);
                        daily_net_values_usd.push(net_value_usd);

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

/// Calculate annualized Sharpe ratio from daily net values.
/// Assumes 365 trading days per year. Returns `None` if there is insufficient data.
fn calculate_sharpe_ratio(daily_values: &[f64], annual_risk_free_rate: f64) -> Option<f64> {
    if daily_values.len() < 3 {
        return None;
    }

    let returns: Vec<f64> = daily_values
        .windows(2)
        .filter(|w| w[0] != 0.0)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    if returns.len() < 2 {
        return None;
    }

    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let daily_risk_free_rate = annual_risk_free_rate / 365.0;
    let excess_return = mean_return - daily_risk_free_rate;

    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return None;
    }

    // Annualize
    Some((excess_return / std_dev) * 365.0_f64.sqrt())
}
