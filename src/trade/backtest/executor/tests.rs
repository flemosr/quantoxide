use crate::util::DateTimeExt;

use super::*;

use chrono::Duration;

use lnm_sdk::api_v3::models::{Leverage, PercentageCapped, Quantity};

fn next_candle(prev: &OhlcCandleRow, price: f64) -> OhlcCandleRow {
    OhlcCandleRow::new_simple(prev.time + Duration::minutes(1), price, prev.volume)
}

#[tokio::test]
async fn test_simulated_trade_executor_long_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 99_000, balance of 1_000_000

    let candle = OhlcCandleRow::new_simple(Utc::now().floor_minute(), 99_000.0, 1_000);
    let start_balance = 1_000_000;
    let config = SimulatedTradeExecutorConfig::default();

    let executor = SimulatedTradeExecutor::new(config, &candle, start_balance);

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

    let opened_trade_id = executor
        .open_long(size, leverage, stoploss, takeprofit)
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

    // Step 2: Open a long trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::try_from(98_000.0).unwrap();
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = None;

    let opened_trade_id = executor
        .open_long(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
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

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::bounded(103_000);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(96_000.));

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.close);
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

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::bounded(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(98_000.));

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
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
        .open_long(size, leverage, stoploss, takeprofit)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
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
        .open_short(size, leverage, stoploss, takeprofit)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().as_f64(), candle.close);
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

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss = None;
    let takeprofit = None;

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.close);
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

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss = None;
    let takeprofit = None;

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.market_price().as_f64(), candle.close);
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

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss_price = Price::bounded(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::bounded(98_000.));

    let opened_trade_id = executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = candle.time + Duration::seconds(59);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), candle.time + Duration::seconds(59));
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
