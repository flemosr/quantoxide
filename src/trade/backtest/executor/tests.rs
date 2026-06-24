use std::num::NonZeroU64;

use crate::{
    db::models::FundingSettlementRow,
    trade::{CrossExposure, CrossQuantity},
    util::DateTimeExt,
};

use super::*;

use chrono::Duration;

use lnm_sdk::api_v3::models::{
    ClientId, CrossLeverage, Leverage, Margin, OrderQuantity, PercentageCapped, SATS_PER_BTC,
    TradeSide,
};

fn next_candle(prev: &OhlcCandleRow, price: f64) -> OhlcCandleRow {
    OhlcCandleRow::new_simple(prev.time + Duration::minutes(1), price, prev.volume)
}

fn next_candle_ohlc(
    prev: &OhlcCandleRow,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
) -> OhlcCandleRow {
    let time = prev.time + Duration::minutes(1);
    OhlcCandleRow {
        time,
        open,
        high,
        low,
        close,
        volume: prev.volume,
        created_at: time,
        updated_at: time,
        stable: true,
    }
}

#[tokio::test]
async fn test_simulated_trade_executor_long_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 99_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 99_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Update market price to 100_000

    let candle = next_candle(&candle, 100_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Open a long trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::bounded(98_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(105_000.));

    let client_id = ClientId::try_from("test-long-profit-001").ok();
    let opened_trade_id = executor
        .open_long(size, leverage, stoploss, takeprofit, client_id.clone())
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(state.running_long_margin() > 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    let (running_trade, _) = state.running_map().get_by_id(opened_trade_id).unwrap();
    assert_eq!(running_trade.client_id(), client_id.as_ref());

    // Step 4: Update price to 101_000

    let candle = next_candle(&candle, 101_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(state.running_long_margin() > 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert!(
        state.running_pl() > 0,
        "Long position should be profitable after price increase"
    );
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close all running long trades
    let closed_trade_ids = executor.close_longs().await?;
    assert_eq!(closed_trade_ids.len(), 1);
    assert_eq!(closed_trade_ids[0], opened_trade_id);

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert!(
        state.realized_pl() > 0,
        "Should have positive PL after closing profitable long"
    );
    assert_eq!(state.closed_len(), 1);
    assert!(state.closed_fees() > 0);

    let closed_trade = state.closed_history().get_by_id(opened_trade_id).unwrap();
    assert_eq!(closed_trade.client_id(), client_id.as_ref());

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_long_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a long trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::try_from(98_000.0).unwrap();
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = None;

    let opened_trade_id = executor
        .open_long(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_long_len(), 1);
    assert!(state.running_long_margin() > 0);
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0);
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 99_000 (1% drop)

    let candle = next_candle(&candle, 99_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(state.running_long_margin() > 0);
    assert!(
        state.running_pl() < 0,
        "Long position should be at a loss after price decrease"
    );
    assert_eq!(state.closed_len(), 0);

    // Step 4: Update price to trigger stoploss (98_000, 2% drop from entry)

    let candle = next_candle(&candle, 98_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert!(
        state.realized_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert_eq!(state.closed_len(), 1);
    assert!(state.closed_fees() > 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::bounded(103_000);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(96_000.));

    let client_id = ClientId::try_from("test-short-profit-001").ok();
    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, client_id.clone())
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    let (running_trade, _) = state.running_map().get_by_id(opened_trade_id).unwrap();
    assert_eq!(running_trade.client_id(), client_id.as_ref());

    // Step 3: Update price to 98_000 (2% drop)

    let candle = next_candle(&candle, 98_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert!(
        state.running_pl() > 0,
        "Short position should be profitable after price decrease"
    );
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Update price to trigger takeprofit (96_000, 4% drop from entry)

    let candle = next_candle(&candle, 96_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 0); // Trade should be closed by takeprofit
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert!(
        state.realized_pl() > 0,
        "Should have positive PL after hitting takeprofit"
    );
    assert_eq!(state.closed_len(), 1);
    assert!(state.closed_fees() > 0);

    let closed_trade = state.closed_history().get_by_id(opened_trade_id).unwrap();
    assert_eq!(closed_trade.client_id(), client_id.as_ref());

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::bounded(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(98_000.));

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0);
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 101_000 (1% increase)

    let candle = next_candle(&candle, 101_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert!(
        state.running_pl() < 0,
        "Short position should be at a loss after price increase"
    );
    assert_eq!(state.closed_len(), 0);

    // Step 4: Update price to trigger stoploss (102_000, 2% increase from entry)

    let candle = next_candle(&candle, 102_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert!(
        state.realized_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert_eq!(state.closed_len(), 1);
    assert!(state.closed_fees() > 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_trailing_stoploss_long() {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into(); // $500 quantity
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_perc = PercentageCapped::try_from(2.0).unwrap();
    let stoploss = Some(Stoploss::trailing(stoploss_perc)); // 2% trailing stop-loss
    let takeprofit = Some(Price::bounded(104_000.));

    // Open long position with trailing stop-loss

    let opened_trade_id = executor
        .open_long(size, leverage, stoploss, takeprofit, None)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), candle.time);
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(trade.stoploss().unwrap().as_f64(), 98_000.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 104_000.0);

    // Verify the opened trade ID matches the running trade
    assert_eq!(trade.id(), opened_trade_id);

    // Price increases to 102_000 (2% increase)
    // Trailing stoploss should move from 98_000 to 99_960 (2% below 102_000)

    let candle = next_candle(&candle, 102_000.0);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_long_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().as_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 104_000.0);

    // Price drops to 99_960.5
    // Should still be above new stop-loss (99_960)

    let candle = next_candle(&candle, 99_960.5);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_long_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().as_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 104_000.0);

    // Price drops to 99_960
    // Should trigger the trailing stop-loss (99_960)
    let candle = next_candle(&candle, 99_960.0);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    assert_eq!(state.running_long_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}

#[tokio::test]
async fn test_simulated_trade_executor_trailing_stoploss_short() {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_perc = PercentageCapped::try_from(2.0).unwrap();
    let stoploss = Some(Stoploss::trailing(stoploss_perc));
    let takeprofit = Some(Price::bounded(96_000.));

    // Open short position with trailing stop-loss

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), candle.time);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(trade.stoploss().unwrap().as_f64(), 102_000.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 96_000.0);

    // Verify the opened trade ID matches the running trade
    assert_eq!(trade.id(), opened_trade_id);

    // Price decreases to 98_000 (2% decrease)
    // Trailing stoploss should move from 102_000 to 99_960 (2% above 98_000)

    let candle = next_candle(&candle, 98_000.0);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_short_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().as_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 96_000.0);

    // Price increases to 99_959.5
    // Should still be below new stop-loss (99_960)

    let candle = next_candle(&candle, 99_959.5);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_short_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().as_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().as_f64(), stoploss_perc.as_f64());
    assert_eq!(trade.takeprofit().unwrap().as_f64(), 96_000.0);

    // Price increases to 99_960
    // Should trigger the trailing stop-loss (99_960)

    let candle = next_candle(&candle, 99_960.0);
    executor.candle_update(&candle).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    assert_eq!(state.running_short_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}

#[tokio::test]
async fn test_simulated_trade_executor_partial_cash_in_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss = None;
    let takeprofit = None;

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)

    let candle = next_candle(&candle, 98_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 500_500);
    assert_eq!(state.running_pl(), 10_204);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Cash_in using the opened trade ID

    let cash_in = 5000;

    executor
        .cash_in(opened_trade_id, cash_in.try_into().unwrap())
        .await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        start_balance - state.running_short_margin() - state.running_fees() + cash_in;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 500_500);
    assert_eq!(state.running_pl(), 10_204 - cash_in as i64);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.realized_pl(), 4_999);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close trade using the opened trade ID

    executor.close_trade(opened_trade_id).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);

    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert!((state.balance() as i64 - expected_balance as i64).abs() < 2);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 10_203);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_fees(), 1_010);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_full_cash_in_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss = None;
    let takeprofit = None;

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_short_len(), 1);
    assert!(state.running_short_margin() > 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)

    let candle = next_candle(&candle, 98_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), 10_204);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Cash_in using the opened trade ID

    let cash_in = 15_000;

    executor
        .cash_in(opened_trade_id, cash_in.try_into().unwrap())
        .await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        start_balance - state.running_short_margin() - state.running_fees() + 10_204;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950 - cash_in + 10_204);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.realized_pl(), 10_204);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close trade using the opened trade ID

    executor.close_trade(opened_trade_id).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);

    let expected_balance = start_balance as i64 + state.realized_pl() - state.closed_fees() as i64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert!((state.balance() as i64 - expected_balance).abs() < 2);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 10_204);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_fees(), 1_010);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_add_margin_short_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().as_f64(), candle.open);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss_price = Price::bounded(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(98_000.));

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert!(state.running_map().get_by_id(opened_trade_id).is_some());
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 101_000 (1% increase)

    let candle = next_candle(&candle, 101_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), -4_951);
    assert_eq!(state.closed_len(), 0);

    // Step 4: Add margin using the opened trade ID

    let add_margin = 5_000;
    executor
        .add_margin(opened_trade_id, add_margin.try_into().unwrap())
        .await?;
    let state = executor.trading_state().await?;

    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950 + add_margin);
    assert_eq!(state.running_pl(), -4_951);
    assert_eq!(state.closed_len(), 0);

    // Step 5: Close trade using the opened trade ID

    executor.close_trade(opened_trade_id).await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);

    let expected_balance =
        (start_balance as i64 + state.realized_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert!((state.balance() as i64 - expected_balance as i64).abs() < 2);
    assert_eq!(state.market_price().as_f64(), candle.close);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.realized_pl(), -4_951);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_fees(), 995);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_long_liquidation_reached() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open a long trade with high leverage and no stoploss
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(50).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_long(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1);

    let (trade, _) = state.running_map().trades_desc().next().unwrap();
    let liquidation = trade.liquidation().as_f64();

    // Candle whose low reaches below liquidation price, but close stays above
    let candle = next_candle_ohlc(&candle, 100_000.0, 100_000.0, liquidation - 1.0, 99_500.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 0, "Long should be liquidated");
    assert_eq!(state.closed_len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_long_liquidation_not_reached() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open a long trade with high leverage and no stoploss
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(50).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_long(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1);

    let (trade, _) = state.running_map().trades_desc().next().unwrap();
    let liquidation = trade.liquidation().as_f64();

    // Candle whose low stays above liquidation price
    let candle = next_candle_ohlc(&candle, 100_000.0, 100_000.0, liquidation + 1.0, 99_500.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1, "Long should still be open");
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_liquidation_reached() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open a short trade with high leverage and no stoploss
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(50).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_short_len(), 1);

    let (trade, _) = state.running_map().trades_desc().next().unwrap();
    let liquidation = trade.liquidation().as_f64();

    // Candle whose high reaches above liquidation price, but close stays below
    let candle = next_candle_ohlc(&candle, 100_000.0, liquidation + 1.0, 100_000.0, 100_500.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_short_len(), 0, "Short should be liquidated");
    assert_eq!(state.closed_len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_liquidation_not_reached() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open a short trade with high leverage and no stoploss
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(50).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_short(size, leverage, stoploss, takeprofit, None)
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_short_len(), 1);

    let (trade, _) = state.running_map().trades_desc().next().unwrap();
    let liquidation = trade.liquidation().as_f64();

    // Candle whose high stays below liquidation price
    let candle = next_candle_ohlc(&candle, 100_000.0, liquidation - 1.0, 100_000.0, 100_500.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_short_len(), 1, "Short should still be open");
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

fn make_settlement(
    time: DateTime<Utc>,
    fixing_price: f64,
    funding_rate: f64,
) -> FundingSettlementRow {
    FundingSettlementRow {
        id: Uuid::new_v4(),
        time,
        fixing_price,
        funding_rate,
        created_at: time,
    }
}

/// Helper: compute the expected funding fee for a given trade side/quantity/settlement,
/// using the same formula as `SimulatedTradeRunning::apply_funding_settlement`.
fn expected_funding_fee(
    side: TradeSide,
    quantity: f64,
    fixing_price: f64,
    funding_rate: f64,
) -> i64 {
    let raw = (quantity / fixing_price) * funding_rate * SATS_PER_BTC;
    match side {
        TradeSide::Buy => raw,
        TradeSide::Sell => -raw,
    }
    .round() as i64
}

/// Positive funding rate: long pays, funding_fees should be positive (cost).
#[tokio::test]
async fn test_funding_settlement_long_positive_rate() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open a long: $10,000 quantity at $60,000
    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();
    assert_eq!(state.funding_fees(), 0);

    // Apply settlement with positive rate (+0.01%)
    // Expected: (10_000 / 60_000) * 0.0001 * 100_000_000 = 1_667 sats
    // Longs pay -> +1_667
    let settlement = make_settlement(candle.time, 60_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    let exp_fee = expected_funding_fee(TradeSide::Buy, 10_000.0, 60_000.0, 0.0001);
    assert!(exp_fee > 0, "Long should pay on positive rate");
    assert_eq!(exp_fee, 1667);
    assert_eq!(state.funding_fees(), exp_fee);
    // Positive fees (cost) are deducted from margin, not from balance directly
    assert_eq!(state.balance(), balance_after_open);
    assert_eq!(state.running_long_len(), 1);

    Ok(())
}

/// Negative funding rate: long receives, funding_fees should be negative (revenue).
#[tokio::test]
async fn test_funding_settlement_long_negative_rate() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();

    // Negative rate: longs receive
    let settlement = make_settlement(candle.time, 60_000.0, -0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    let exp_fee = expected_funding_fee(TradeSide::Buy, 10_000.0, 60_000.0, -0.0001);
    assert!(exp_fee < 0, "Long should receive on negative rate");
    assert_eq!(exp_fee, -1667);
    assert_eq!(state.funding_fees(), exp_fee);
    // Negative fees (revenue) are added to balance
    assert_eq!(state.balance(), balance_after_open + (-exp_fee) as u64);
    assert_eq!(state.running_long_len(), 1);

    Ok(())
}

/// Positive funding rate: short receives, funding_fees should be negative (revenue).
#[tokio::test]
async fn test_funding_settlement_short_positive_rate() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();

    // Positive rate: shorts receive
    let settlement = make_settlement(candle.time, 60_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    let exp_fee = expected_funding_fee(TradeSide::Sell, 10_000.0, 60_000.0, 0.0001);
    assert!(exp_fee < 0, "Short should receive on positive rate");
    assert_eq!(exp_fee, -1667);
    assert_eq!(state.funding_fees(), exp_fee);
    assert_eq!(state.balance(), balance_after_open + (-exp_fee) as u64);
    assert_eq!(state.running_short_len(), 1);

    Ok(())
}

/// Negative funding rate: short pays, funding_fees should be positive (cost).
#[tokio::test]
async fn test_funding_settlement_short_negative_rate() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();

    let settlement = make_settlement(candle.time, 60_000.0, -0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    let exp_fee = expected_funding_fee(TradeSide::Sell, 10_000.0, 60_000.0, -0.0001);
    assert!(exp_fee > 0, "Short should pay on negative rate");
    assert_eq!(exp_fee, 1667);
    assert_eq!(state.funding_fees(), exp_fee);
    // Positive fees (cost) deducted from margin, balance unchanged
    assert_eq!(state.balance(), balance_after_open);
    assert_eq!(state.running_short_len(), 1);

    Ok(())
}

/// Multiple settlements accumulate funding_fees correctly.
#[tokio::test]
async fn test_funding_settlement_cumulative_fees() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open long $500 at $100k
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    // Settlement 1: positive rate -> long pays
    let s1 = make_settlement(candle.time, 100_000.0, 0.0001);
    executor.apply_funding_settlement(&s1).await?;

    let fee1 = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.0001);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee1);

    // Settlement 2: negative rate -> long receives
    let s2 = make_settlement(candle.time + Duration::hours(8), 100_000.0, -0.0002);
    executor.apply_funding_settlement(&s2).await?;

    let fee2 = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, -0.0002);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee1 + fee2);

    // Settlement 3: another positive rate
    let s3 = make_settlement(candle.time + Duration::hours(16), 100_000.0, 0.00005);
    executor.apply_funding_settlement(&s3).await?;

    let fee3 = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.00005);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee1 + fee2 + fee3);

    Ok(())
}

/// Settlement with no open positions is a no-op.
#[tokio::test]
async fn test_funding_settlement_no_positions() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let settlement = make_settlement(candle.time, 100_000.0, 0.001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), start_balance);

    Ok(())
}

/// Funding fee with zero rate should produce zero fee.
#[tokio::test]
async fn test_funding_settlement_zero_rate() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();

    let settlement = make_settlement(candle.time, 100_000.0, 0.0);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), balance_after_open);

    Ok(())
}

/// Funding applied to multiple open positions (long + short) simultaneously.
#[tokio::test]
async fn test_funding_settlement_mixed_positions() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();

    executor.open_long(size, leverage, None, None, None).await?;
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let state = executor.trading_state().await?;
    let balance_after_open = state.balance();
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.running_short_len(), 1);

    // Positive rate: long pays, short receives. With same quantity, fees nearly cancel out.
    let settlement = make_settlement(candle.time, 100_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let long_fee = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.0001);
    let short_fee = expected_funding_fee(TradeSide::Sell, 500.0, 100_000.0, 0.0001);
    assert_eq!(long_fee, -short_fee, "Fees should be equal and opposite");

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), long_fee + short_fee);
    // Net fee is 0 (cancel out), but negative short fee (revenue) was added to balance
    assert_eq!(state.balance(), balance_after_open + (-short_fee) as u64);
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.running_short_len(), 1);

    Ok(())
}

/// Negative funding fees reduce margin, leverage, and shift liquidation price.
#[tokio::test]
async fn test_funding_settlement_margin_reduction_long() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let (trade_before, _) = state.running_map().trades_desc().next().unwrap();
    let margin_before = trade_before.margin();
    let liquidation_before = trade_before.liquidation();

    // Apply a large positive rate so the long pays a significant fee
    let settlement = make_settlement(candle.time, 100_000.0, 0.01); // 1%
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1);

    let (trade_after, _) = state.running_map().trades_desc().next().unwrap();
    assert!(
        trade_after.margin().as_u64() < margin_before.as_u64(),
        "Margin should decrease when long pays funding fee"
    );
    assert!(
        trade_after.liquidation().as_f64() > liquidation_before.as_f64(),
        "Liquidation price should move closer to market when margin decreases (long)"
    );

    Ok(())
}

/// Negative funding fees reduce margin for shorts as well.
#[tokio::test]
async fn test_funding_settlement_margin_reduction_short() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let state = executor.trading_state().await?;
    let (trade_before, _) = state.running_map().trades_desc().next().unwrap();
    let margin_before = trade_before.margin();
    let liquidation_before = trade_before.liquidation();

    // Large negative rate so the short pays
    let settlement = make_settlement(candle.time, 100_000.0, -0.01); // -1%
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_short_len(), 1);

    let (trade_after, _) = state.running_map().trades_desc().next().unwrap();
    assert!(
        trade_after.margin().as_u64() < margin_before.as_u64(),
        "Margin should decrease when short pays funding fee"
    );
    assert!(
        trade_after.liquidation().as_f64() < liquidation_before.as_f64(),
        "Liquidation price should move closer to market when margin decreases (short)"
    );

    Ok(())
}

/// Funding fees are reflected in final PL when closing a trade.
#[tokio::test]
async fn test_funding_settlement_reflected_in_close() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    // Short receives positive rate -> funding_fees negative (revenue)
    let settlement = make_settlement(candle.time, 100_000.0, 0.001); // 0.1%
    executor.apply_funding_settlement(&settlement).await?;

    let state_before_close = executor.trading_state().await?;
    let funding_fees_before = state_before_close.funding_fees();
    assert!(funding_fees_before < 0);

    // Close at the same price (no price PL)
    executor.close_shorts().await?;

    let state = executor.trading_state().await?;
    // Funding fees should persist after closing
    assert_eq!(state.funding_fees(), funding_fees_before);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.closed_len(), 1);

    Ok(())
}

/// Verify the LN Markets documentation example:
/// +0.01% rate, $10,000 quantity at $60,000 BTCUSD -> 1,667 sats.
#[tokio::test]
async fn test_funding_settlement_docs_example() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Open long with $10,000 quantity
    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    // +0.01% = 0.0001
    let settlement = make_settlement(candle.time, 60_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    // (10_000 / 60_000) * 0.0001 * 100_000_000 = 1_666.666... -> rounds to 1_667
    // Long pays -> +1_667 (positive = cost in market convention)
    assert_eq!(state.funding_fees(), 1667);

    Ok(())
}

/// Verify the same docs example but for a short position (short receives).
#[tokio::test]
async fn test_funding_settlement_docs_example_short() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 60_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let settlement = make_settlement(candle.time, 60_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    // Short receives -> -1_667 (negative = revenue in market convention)
    assert_eq!(state.funding_fees(), -1667);

    Ok(())
}

/// Funding settlement applied to a leveraged long position.
#[tokio::test]
async fn test_funding_settlement_leveraged_long() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // $500 quantity with 10x leverage (so margin is ~$50 worth)
    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let (trade_before, _) = state.running_map().trades_desc().next().unwrap();
    let margin_before = trade_before.margin();
    let leverage_before = trade_before.leverage();

    // Positive rate -> long pays
    let settlement = make_settlement(candle.time, 100_000.0, 0.001); // 0.1%
    executor.apply_funding_settlement(&settlement).await?;

    // Fee = (500 / 100_000) * 0.001 * 100_000_000 = 500 sats
    let exp_fee = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.001);
    assert_eq!(exp_fee, 500);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), exp_fee);
    assert_eq!(state.running_long_len(), 1);

    let (trade_after, _) = state.running_map().trades_desc().next().unwrap();
    // Margin should be reduced by fee amount
    assert_eq!(
        trade_after.margin().as_i64(),
        margin_before.as_i64() - exp_fee
    );
    // With lower margin and same quantity, effective leverage increases
    assert!(trade_after.leverage() >= leverage_before);

    Ok(())
}

/// Funding settlement does not affect balance when position is closed before settlement.
#[tokio::test]
async fn test_funding_settlement_after_close_is_noop() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    // Close the trade first
    executor.close_longs().await?;

    let state = executor.trading_state().await?;
    let balance_after_close = state.balance();
    assert_eq!(state.running_long_len(), 0);

    // Settlement after close should be a no-op
    let settlement = make_settlement(candle.time, 100_000.0, 0.01);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), balance_after_close);

    Ok(())
}

/// Funding settlement with different fixing price changes fee magnitude.
#[tokio::test]
async fn test_funding_settlement_fixing_price_impact() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(10_000).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    // Lower fixing price -> more BTC notional -> larger fee
    let settlement_low = make_settlement(candle.time, 50_000.0, 0.0001);
    executor.apply_funding_settlement(&settlement_low).await?;

    let fee_low_fixing = expected_funding_fee(TradeSide::Buy, 10_000.0, 50_000.0, 0.0001);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee_low_fixing);
    // (10_000 / 50_000) * 0.0001 * 100_000_000 = 2_000 sats -> long pays +2_000
    assert_eq!(fee_low_fixing, 2000);

    Ok(())
}

/// Multiple settlements across multiple trades with price changes in between.
#[tokio::test]
async fn test_funding_settlement_with_price_movement() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    // Apply first settlement
    let s1 = make_settlement(candle.time, 100_000.0, 0.0001);
    executor.apply_funding_settlement(&s1).await?;

    let fee1 = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.0001);

    // Price moves up
    let candle = next_candle(&candle, 105_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee1);
    assert!(
        state.running_pl() > 0,
        "Should be profitable after price increase"
    );

    // Apply second settlement at new fixing price
    let s2 = make_settlement(candle.time, 105_000.0, 0.0002);
    executor.apply_funding_settlement(&s2).await?;

    let fee2 = expected_funding_fee(TradeSide::Buy, 500.0, 105_000.0, 0.0002);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), fee1 + fee2);
    assert!(state.running_pl() > 0);
    assert_eq!(state.running_long_len(), 1);

    Ok(())
}

/// Repeated settlements progressively erode margin on leveraged trade.
#[tokio::test]
async fn test_funding_settlement_progressive_margin_erosion() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    let state = executor.trading_state().await?;
    let (trade0, _) = state.running_map().trades_desc().next().unwrap();
    let initial_margin = trade0.margin().as_u64();

    let mut cumulative_fees = 0i64;

    // Apply 5 successive settlements with positive rate (long pays each time)
    for i in 0..5 {
        let settlement = make_settlement(
            candle.time + Duration::hours(8 * i),
            100_000.0,
            0.001, // 0.1%
        );
        executor.apply_funding_settlement(&settlement).await?;

        let fee = expected_funding_fee(TradeSide::Buy, 500.0, 100_000.0, 0.001);
        cumulative_fees += fee;

        let state = executor.trading_state().await?;
        assert_eq!(state.funding_fees(), cumulative_fees);

        if state.running_long_len() == 1 {
            let (trade, _) = state.running_map().trades_desc().next().unwrap();
            assert!(
                trade.margin().as_u64() < initial_margin,
                "Margin should erode after {} settlements",
                i + 1
            );
        }
    }

    Ok(())
}

/// Positive funding fee received by short does not alter trade margin (only balance).
#[tokio::test]
async fn test_funding_settlement_positive_fee_no_margin_change() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    let size = OrderQuantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor
        .open_short(size, leverage, None, None, None)
        .await?;

    let state = executor.trading_state().await?;
    let (trade_before, _) = state.running_map().trades_desc().next().unwrap();
    let margin_before = trade_before.margin();
    let leverage_before = trade_before.leverage();
    let liquidation_before = trade_before.liquidation();

    // Positive rate -> short receives -> trade margin/leverage/liquidation unchanged
    let settlement = make_settlement(candle.time, 100_000.0, 0.001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    let (trade_after, _) = state.running_map().trades_desc().next().unwrap();
    assert_eq!(trade_after.margin(), margin_before);
    assert_eq!(trade_after.leverage(), leverage_before);
    assert_eq!(
        trade_after.liquidation().as_f64(),
        liquidation_before.as_f64()
    );

    Ok(())
}

/// Margin-sized 1x trades may float below `Leverage::MIN` since the open-time quantity is floored.
/// Funding settlement must clamp such sub-MIN recomputations to `MIN` rather than force-close,
/// matching LN Markets' acceptance of overcollateralized positions.
#[tokio::test]
async fn test_funding_settlement_margin_sized_1x_not_force_closed() -> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 42_879.0, 1_000);
    let start_balance = 100_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

    // Margin-sized 1x long: quantity = floor(99_876 * 1 * 42_879 / 1e8) = floor(42.83) = 42,
    // giving economic leverage 42 * 1e8 / (99_876 * 42_879) ≈ 0.9807 — below `Leverage::MIN`.
    let size = Margin::try_from(99_876_u64).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    executor.open_long(size, leverage, None, None, None).await?;

    assert_eq!(executor.trading_state().await?.running_long_len(), 1);

    // Positive rate -> long pays -> settlement recomputes leverage from the floored quantity.
    let settlement = make_settlement(candle.time, 42_879.0, 0.0001);
    executor.apply_funding_settlement(&settlement).await?;

    let state = executor.trading_state().await?;
    assert_eq!(
        state.running_long_len(),
        1,
        "Trade must survive the settlement; sub-MIN economic leverage should clamp to `MIN`",
    );
    assert_eq!(state.closed_history().len(), 0);

    let (trade, _) = state.running_map().trades_desc().next().unwrap();
    assert_eq!(
        trade.leverage(),
        Leverage::MIN,
        "Post-settlement leverage should be clamped to `Leverage::MIN`",
    );

    Ok(())
}

async fn seed_cross_position(
    executor: &SimulatedTradeExecutor,
    margin: u64,
    leverage: CrossLeverage,
    side: TradeSide,
    quantity: impl Into<CrossQuantity>,
    entry_price: Price,
) {
    let mut state_guard = executor.state.lock().await;
    state_guard.cross_position = SimulatedCrossPosition::new(
        margin,
        leverage,
        Some((side, quantity.into(), entry_price)),
        0,
        0,
        0,
    )
    .expect("seeded cross position must be valid");
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_deposit_moves_balance_to_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    let cross_position = executor
        .cross_deposit(NonZeroU64::new(100_000).unwrap())
        .await?;
    assert_eq!(cross_position.margin(), 100_000);

    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), 900_000);
    assert_eq!(state.cross_position().margin(), 100_000);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        100_000
    );
    assert_eq!(
        state.cross_position().est_net_value(state.market_price()),
        100_000
    );
    assert_eq!(state.cross_position().quantity(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_deposit_rejects_insufficient_balance()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    let result = executor
        .cross_deposit(NonZeroU64::new(1_000_001).unwrap())
        .await;

    assert!(result.is_err());
    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.cross_position().margin(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_withdraw_moves_margin_to_balance()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    executor
        .cross_deposit(NonZeroU64::new(100_000).unwrap())
        .await?;
    let cross_position = executor
        .cross_withdraw(NonZeroU64::new(40_000).unwrap())
        .await?;
    assert_eq!(cross_position.margin(), 60_000);

    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), 940_000);
    assert_eq!(state.cross_position().margin(), 60_000);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        60_000
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_withdraw_rejects_amount_above_free_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    executor
        .cross_deposit(NonZeroU64::new(100_000).unwrap())
        .await?;
    let result = executor
        .cross_withdraw(NonZeroU64::new(100_001).unwrap())
        .await;

    assert!(result.is_err());
    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), 900_000);
    assert_eq!(state.cross_position().margin(), 100_000);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_withdraw_respects_nav_constraint()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    seed_cross_position(
        &executor,
        113_000,
        CrossLeverage::try_from(10).unwrap(),
        TradeSide::Buy,
        OrderQuantity::try_from(1_000).unwrap(),
        Price::bounded(100_000.0),
    )
    .await;
    executor
        .candle_update(&next_candle(&candle, 90_000.0))
        .await?;

    // Add one sat so the fixture has exactly 389 sats of estimated free margin after the loss.
    executor.cross_deposit(NonZeroU64::new(1).unwrap()).await?;

    let state = executor.trading_state().await?;
    assert!(state.cross_position().est_running_pl(state.market_price()) < 0);
    assert!(
        state
            .cross_position()
            .est_running_pl(state.market_price())
            .unsigned_abs()
            > state.cross_position().running_margin()
    );
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        389
    );

    let result = executor.cross_withdraw(NonZeroU64::new(390).unwrap()).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_set_leverage_persists_while_flat()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );
    let leverage = CrossLeverage::try_from(10).unwrap();

    let cross_position = executor.cross_set_leverage(leverage).await?;
    assert_eq!(cross_position.leverage(), leverage);

    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.cross_position().margin(), 0);
    assert_eq!(state.cross_position().leverage(), leverage);
    assert_eq!(state.cross_position().running_margin(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_set_leverage_reallocates_running_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );
    let entry_price = Price::bounded(100_000.0);

    seed_cross_position(
        &executor,
        500_000,
        CrossLeverage::try_from(10).unwrap(),
        TradeSide::Buy,
        OrderQuantity::try_from(1_000).unwrap(),
        entry_price,
    )
    .await;
    executor
        .cross_set_leverage(CrossLeverage::try_from(20).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 500_000);
    assert_eq!(state.cross_position().running_margin(), 50_000);
    assert_eq!(state.cross_position().maintenance_margin(), 1_500);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        448_500
    );
    assert_eq!(state.cross_position().entry_price(), Some(entry_price));

    executor
        .cross_set_leverage(CrossLeverage::try_from(5).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 500_000);
    assert_eq!(state.cross_position().running_margin(), 200_000);
    assert_eq!(state.cross_position().maintenance_margin(), 1_500);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        298_500
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_set_leverage_rejects_insufficient_free_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );
    let leverage = CrossLeverage::try_from(10).unwrap();

    seed_cross_position(
        &executor,
        101_501,
        leverage,
        TradeSide::Buy,
        OrderQuantity::try_from(1_000).unwrap(),
        Price::bounded(100_000.0),
    )
    .await;
    let result = executor
        .cross_set_leverage(CrossLeverage::try_from(5).unwrap())
        .await;

    assert!(result.is_err());
    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().leverage(), leverage);
    assert_eq!(state.cross_position().running_margin(), 100_000);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        1
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_market_opens_long_position() -> TradeExecutorResult<()>
{
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let start_balance = 1_000_000;
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        start_balance,
    );

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market(TradeSide::Buy, OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.balance(), 500_000);
    assert_eq!(state.last_trade_time(), Some(candle.time));
    assert_eq!(state.cross_position().margin(), 499_000);
    assert_eq!(state.cross_position().quantity(), 1_000);
    assert_eq!(
        state.cross_position().entry_price(),
        Some(Price::bounded(100_000.0))
    );
    assert_eq!(state.cross_position().running_margin(), 100_000);
    assert_eq!(state.cross_position().maintenance_margin(), 1_500);
    assert_eq!(
        state.cross_position().exposure(),
        CrossExposure::running(
            state.cross_position().margin(),
            state.cross_position().leverage(),
            TradeSide::Buy,
            CrossQuantity::try_from(1_000).unwrap(),
            Price::bounded(100_000.0),
        )
        .unwrap()
    );
    assert_eq!(state.cross_position().trading_fees(), 1_000);
    assert_eq!(
        state.cross_position().est_free_margin(state.market_price()),
        397_500
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_exposure_quantity_can_exceed_order_quantity_limit()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor = SimulatedTradeExecutor::new(
        SimulatedTradeExecutorConfig::default(),
        &candle,
        250_000_000,
    );

    assert!(OrderQuantity::try_from(500_001).is_err());
    assert!(CrossQuantity::try_from(1_000_000).is_ok());
    assert!(CrossQuantity::try_from(CrossQuantity::HARD_MAX.as_u64()).is_ok());
    assert!(CrossQuantity::try_from(CrossQuantity::HARD_MAX.as_u64() + 1).is_err());

    executor
        .cross_deposit(NonZeroU64::new(200_000_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor.cross_market_long(OrderQuantity::MAX).await?;
    executor.cross_market_long(OrderQuantity::MAX).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().quantity(), 1_000_000);
    assert_eq!(state.cross_position().running_margin(), 100_000_000);
    assert_eq!(state.cross_position().maintenance_margin(), 1_500_000);
    assert_eq!(state.cross_position().trading_fees(), 1_000_000);
    assert_eq!(
        state.cross_position().exposure(),
        CrossExposure::running(
            state.cross_position().margin(),
            state.cross_position().leverage(),
            TradeSide::Buy,
            CrossQuantity::try_from(1_000_000).unwrap(),
            Price::bounded(100_000.0),
        )
        .unwrap()
    );

    let close_id = executor.cross_close_position().await?;
    let state = executor.trading_state().await?;
    assert!(close_id.is_some());
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);
    assert_eq!(state.cross_position().margin(), 198_000_000);
    assert_eq!(state.cross_position().trading_fees(), 2_000_000);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_market_same_side_aggregates_entry()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    let candle = next_candle(&candle, 200_000.0);
    executor.candle_update(&candle).await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 498_500);
    assert_eq!(state.cross_position().quantity(), 2_000);
    assert!((state.cross_position().entry_price().unwrap().as_f64() - 133_333.5).abs() <= 0.5);
    assert_eq!(state.cross_position().running_margin(), 149_999);
    assert_eq!(state.cross_position().trading_fees(), 1_500);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_market_profitable_partial_reduce_books_pl()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    let candle = next_candle(&candle, 110_000.0);
    executor.candle_update(&candle).await?;
    executor
        .cross_market_short(OrderQuantity::try_from(400).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 535_000);
    assert_eq!(state.cross_position().quantity(), 600);
    assert_eq!(
        state.cross_position().entry_price(),
        Some(Price::bounded(100_000.0))
    );
    assert_eq!(state.cross_position().realized_pl(), 36_363);
    assert_eq!(state.cross_position().trading_fees(), 1_363);
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_market_losing_partial_reduce_carries_loss()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    let candle = next_candle(&candle, 90_000.0);
    executor.candle_update(&candle).await?;
    executor
        .cross_market_short(OrderQuantity::try_from(400).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 498_556);
    assert_eq!(state.cross_position().quantity(), 600);
    assert_eq!(
        state.cross_position().entry_price(),
        Some(Price::bounded(108_000.0))
    );
    assert_eq!(
        state.cross_position().est_running_pl(state.market_price()),
        -111_112
    );
    assert_eq!(state.cross_position().realized_pl(), 0);
    assert_eq!(state.cross_position().trading_fees(), 1_444);
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_close_position_books_pl_and_flattens()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    let candle = next_candle(&candle, 110_000.0);
    executor.candle_update(&candle).await?;
    let close_id = executor.cross_close_position().await?;

    let state = executor.trading_state().await?;
    assert!(close_id.is_some());
    assert_eq!(state.cross_position().margin(), 589_000);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.cross_position().entry_price(), None);
    assert_eq!(state.cross_position().running_margin(), 0);
    assert_eq!(state.cross_position().maintenance_margin(), 0);
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);
    assert_eq!(state.cross_position().realized_pl(), 90_909);
    assert_eq!(state.cross_position().trading_fees(), 1_909);
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_market_reversal_books_pl_and_resets_entry()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    let candle = next_candle(&candle, 90_000.0);
    executor.candle_update(&candle).await?;
    executor
        .cross_market_short(OrderQuantity::try_from(1_500).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().margin(), 386_222);
    assert_eq!(state.cross_position().quantity(), -500);
    assert_eq!(
        state.cross_position().entry_price(),
        Some(Price::bounded(90_000.0))
    );
    assert_eq!(state.cross_position().realized_pl(), -111_112);
    assert_eq!(state.cross_position().trading_fees(), 2_666);
    assert_eq!(
        state.cross_position().est_running_pl(state.market_price()),
        0
    );
    assert_eq!(state.closed_len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_close_position_returns_none_when_flat()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    let close_id = executor.cross_close_position().await?;

    let state = executor.trading_state().await?;
    assert_eq!(close_id, None);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_orders_coexist_with_isolated_trades()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .open_long(
            OrderQuantity::try_from(100).unwrap().into(),
            Leverage::try_from(1).unwrap(),
            None,
            None,
            None,
        )
        .await?;
    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;
    executor.cross_close_position().await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.cross_position().margin(), 498_000);
    assert_eq!(state.cross_position().trading_fees(), 2_000);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_funding_long_pays_from_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    let margin_before = state.cross_position().margin();
    let liquidation_before = state.cross_position().liquidation().unwrap();
    let running_pl_before = state.cross_position().est_running_pl(state.market_price());

    let settlement = make_settlement(candle.time, 100_000.0, 0.01);
    executor.apply_funding_settlement(&settlement).await?;

    let expected_fee = expected_funding_fee(TradeSide::Buy, 1_000.0, 100_000.0, 0.01);
    assert_eq!(expected_fee, 10_000);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), 500_000);
    assert_eq!(
        state.cross_position().margin(),
        margin_before - expected_fee as u64
    );
    assert_eq!(
        state.cross_position().est_running_pl(state.market_price()),
        running_pl_before
    );
    assert!(state.cross_position().liquidation().unwrap() > liquidation_before);

    assert_eq!(state.cross_position().session_funding_fees(), expected_fee);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_funding_cost_can_force_flatten_profitable_position()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let candle = next_candle(&candle, 110_000.0);
    executor.candle_update(&candle).await?;
    let state = executor.trading_state().await?;
    assert!(state.cross_position().est_running_pl(state.market_price()) > 0);

    let settlement = make_settlement(candle.time + Duration::hours(8), 100_000.0, 0.5);
    executor.apply_funding_settlement(&settlement).await?;

    let expected_fee = expected_funding_fee(TradeSide::Buy, 1_000.0, 100_000.0, 0.5);
    assert_eq!(expected_fee, 500_000);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), 500_000);
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);
    assert_eq!(state.cross_position().margin(), 89_000);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.last_trade_time(), Some(settlement.time));
    assert_eq!(state.cross_position().trading_fees(), 1_909);
    assert_eq!(state.total_net_value(), state.balance() + 89_000);

    assert_eq!(state.cross_position().session_funding_fees(), expected_fee);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_funding_short_receives_to_margin()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_short(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    let margin_before = state.cross_position().margin();
    let liquidation_before = state.cross_position().liquidation().unwrap();

    let settlement = make_settlement(candle.time, 100_000.0, 0.01);
    executor.apply_funding_settlement(&settlement).await?;

    let expected_fee = expected_funding_fee(TradeSide::Sell, 1_000.0, 100_000.0, 0.01);
    assert_eq!(expected_fee, -10_000);

    let state = executor.trading_state().await?;
    assert_eq!(state.funding_fees(), 0);
    assert_eq!(state.balance(), 500_000);
    assert_eq!(
        state.cross_position().margin(),
        margin_before + expected_fee.unsigned_abs()
    );
    assert!(state.cross_position().liquidation().unwrap() > liquidation_before);

    assert_eq!(state.cross_position().session_funding_fees(), expected_fee);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_long_liquidates_on_candle_low()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    let liquidation = state.cross_position().liquidation().unwrap().as_f64();
    let candle = next_candle_ohlc(&candle, 100_000.0, 100_000.0, liquidation - 1.0, 90_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);
    assert_eq!(state.cross_position().margin(), 3);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(
        state.last_trade_time(),
        Some(candle.time + Duration::seconds(59))
    );
    assert_eq!(
        state.total_net_value(),
        state.balance() + state.cross_position().margin()
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_cross_short_liquidates_on_candle_high()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 1_000_000);

    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_short(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let state = executor.trading_state().await?;
    let liquidation = state.cross_position().liquidation().unwrap().as_f64();
    let candle = next_candle_ohlc(&candle, 100_000.0, liquidation + 1.0, 100_000.0, 110_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.cross_position().exposure(), CrossExposure::Neutral);
    assert_eq!(state.cross_position().margin(), 997);
    assert_eq!(state.cross_position().quantity(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(
        state.last_trade_time(),
        Some(candle.time + Duration::seconds(59))
    );
    assert_eq!(
        state.total_net_value(),
        state.balance() + state.cross_position().margin()
    );

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_total_net_value_includes_isolated_and_cross_exposure()
-> TradeExecutorResult<()> {
    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 100_000.0, 1_000);
    let executor =
        SimulatedTradeExecutor::new(SimulatedTradeExecutorConfig::default(), &candle, 2_000_000);

    executor
        .open_long(
            OrderQuantity::try_from(100).unwrap().into(),
            Leverage::try_from(1).unwrap(),
            None,
            None,
            None,
        )
        .await?;
    executor
        .cross_deposit(NonZeroU64::new(500_000).unwrap())
        .await?;
    executor
        .cross_set_leverage(CrossLeverage::try_from(10).unwrap())
        .await?;
    executor
        .cross_market_long(OrderQuantity::try_from(1_000).unwrap())
        .await?;

    let candle = next_candle(&candle, 110_000.0);
    executor.candle_update(&candle).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.cross_position().quantity(), 1_000);
    assert!(state.running_pl() > 0);
    assert!(state.cross_position().est_running_pl(state.market_price()) > 0);
    assert_eq!(
        state.total_net_value(),
        state
            .balance()
            .saturating_add(state.running_margin())
            .saturating_add_signed(state.running_pl())
            .saturating_add(state.cross_position().est_net_value(state.market_price()))
    );

    Ok(())
}
