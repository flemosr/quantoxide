use crate::{db::models::FundingSettlementRow, util::DateTimeExt};

use super::*;

use chrono::Duration;

use lnm_sdk::api_v3::models::{ClientId, Leverage, PercentageCapped, Quantity, SATS_PER_BTC};

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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into(); // $500 quantity
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(10_000).unwrap().into();
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

    let size = Quantity::try_from(10_000).unwrap().into();
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

    let size = Quantity::try_from(10_000).unwrap().into();
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

    let size = Quantity::try_from(10_000).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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
    let size = Quantity::try_from(10_000).unwrap().into();
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

    let size = Quantity::try_from(10_000).unwrap().into();
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
    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(10_000).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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

    let size = Quantity::try_from(500).unwrap().into();
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
