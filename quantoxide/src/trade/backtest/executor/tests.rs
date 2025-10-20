use super::*;

use chrono::Duration;

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, Quantity};

#[tokio::test]
async fn test_simulated_trade_executor_long_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 99_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 99_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap();
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Update market price to 100_000

    let time = start_time + Duration::seconds(1);
    let market_price = 100_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Open a long trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::clamp_from(98_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::clamp_from(105_000.));

    executor
        .open_long(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = time;
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Update price to 101_000

    let time = time + Duration::seconds(1);
    let market_price = 101_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert!(
        state.running_pl() > 0,
        "Long position should be profitable after price increase"
    );
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close all running long trades
    executor.close_longs().await?;

    let state = executor.trading_state().await?;
    let exp_trade_time = time;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert!(
        state.closed_pl() > 0,
        "Should have positive PL after closing profitable long"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_long_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap();
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a long trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::try_from(98_000.0).unwrap();
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = None;

    executor
        .open_long(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 99_000 (1% drop)

    let time = start_time + Duration::seconds(1);
    let market_price = 99_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert!(
        state.running_pl() < 0,
        "Long position should be at a loss after price decrease"
    );
    assert_eq!(state.closed_len(), 0);

    // Step 4: Update price to trigger stoploss (98_000, 2% drop from entry)

    let time = time + Duration::seconds(1);
    let market_price = 98_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert!(
        state.closed_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::clamp_from(103_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::clamp_from(96_000.));

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)

    let time = start_time + Duration::seconds(1);
    let market_price = 98_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert!(
        state.running_pl() > 0,
        "Short position should be profitable after price decrease"
    );
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Update price to trigger takeprofit (96_000, 4% drop from entry)

    let time = time + Duration::seconds(1);
    let market_price = 96_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 0); // Trade should be closed by takeprofit
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert!(
        state.closed_pl() > 0,
        "Should have positive PL after hitting takeprofit"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_short_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap();
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_price = Price::clamp_from(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::clamp_from(98_000.));

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 101_000 (1% increase)

    let time = start_time + Duration::seconds(1);
    let market_price = 101_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert!(
        state.running_pl() < 0,
        "Short position should be at a loss after price increase"
    );
    assert_eq!(state.closed_len(), 0);

    // Step 4: Update price to trigger stoploss (102_000, 2% increase from entry)

    let time = time + Duration::seconds(1);
    let market_price = 102_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert!(
        state.closed_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_trailing_stoploss_long() {
    let start_time = Utc::now();
    let start_balance = 100_000_000;
    let market_price = 100_000.0;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let size = Quantity::try_from(500).unwrap().into(); // $500 quantity
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap();
    let stoploss = Some(Stoploss::trailing(stoploss_perc)); // 2% trailing stop-loss
    let takeprofit = Some(Price::clamp_from(104_000.));

    // Open long position with trailing stop-loss

    executor
        .open_long(size, leverage, stoploss, takeprofit)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), start_time);
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(trade.stoploss().unwrap().into_f64(), 98_000.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 104_000.0);

    // Price increases to 102_000 (2% increase)
    // Trailing stoploss should move from 98_000 to 99_960 (2% below 102_000)

    let time = start_time + chrono::Duration::seconds(1);
    executor.tick_update(time, 102_000.0).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_long_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().into_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 104_000.0);

    // Price drops to 99_960.5
    // Should still be above new stop-loss (99_960)

    let time = time + chrono::Duration::seconds(1);
    executor.tick_update(time, 99_960.5).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_long_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().into_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 104_000.0);

    // Price drops to 99_960
    // Should trigger the trailing stop-loss (99_960)
    let time = time + chrono::Duration::seconds(1);
    executor.tick_update(time, 99_960.0).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    assert_eq!(state.running_long_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}

#[tokio::test]
async fn test_simulated_trade_executor_trailing_stoploss_short() {
    let start_time = Utc::now();
    let start_balance = 100_000_000;
    let market_price = 100_000.0;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap();
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap();
    let stoploss = Some(Stoploss::trailing(stoploss_perc));
    let takeprofit = Some(Price::clamp_from(96_000.));

    // Open short position with trailing stop-loss

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await
        .unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.last_tick_time(), start_time);
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(trade.stoploss().unwrap().into_f64(), 102_000.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 96_000.0);

    // Price decreases to 98_000 (2% decrease)
    // Trailing stoploss should move from 102_000 to 99_960 (2% above 98_000)

    let time = start_time + chrono::Duration::seconds(1);
    executor.tick_update(time, 98_000.0).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_short_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().into_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 96_000.0);

    // Price increases to 99_959.5
    // Should still be below new stop-loss (99_960)

    let time = time + chrono::Duration::seconds(1);
    executor.tick_update(time, 99_959.5).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    let Some((trade, tsl)) = state.running_map().trades_desc().next() else {
        panic!("must have trade");
    };

    assert_eq!(state.running_short_len(), 1); // Position still open
    assert_eq!(trade.stoploss().unwrap().into_f64(), 99_960.0);
    assert_eq!(tsl.unwrap().into_f64(), stoploss_perc.into_f64());
    assert_eq!(trade.takeprofit().unwrap().into_f64(), 96_000.0);

    // Price increases to 99_960
    // Should trigger the trailing stop-loss (99_960)

    let time = time + chrono::Duration::seconds(1);
    executor.tick_update(time, 99_960.0).await.unwrap();

    let state = executor.trading_state().await.unwrap();
    assert_eq!(state.running_short_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}

#[tokio::test]
async fn test_simulated_trade_executor_partial_cash_in_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)

    let time = start_time + Duration::seconds(1);
    let market_price = 98_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 500_500);
    assert_eq!(state.running_pl(), 10_204);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Cash_in

    let state = executor.trading_state().await?;
    let (trade, _) = state
        .running_map()
        .trades_desc()
        .next()
        .expect("has running trade");

    let cash_in = 5000;

    executor
        .cash_in(trade.id(), cash_in.try_into().unwrap())
        .await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        start_balance - state.running_short_margin() - state.running_fees() + cash_in;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 500_500);
    assert_eq!(state.running_pl(), 10_204 - cash_in as i64);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 4_999);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close trade

    executor.close_trade(trade.id()).await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert!((state.balance() as i64 - expected_balance as i64).abs() < 2);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_pl(), 10_203);
    assert_eq!(state.closed_fees(), 1_010);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_full_cash_in_short_profit() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss = None;
    let takeprofit = None;

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)

    let time = start_time + Duration::seconds(1);
    let market_price = 98_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), 10_204);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Cash_in

    let state = executor.trading_state().await?;
    let (trade, _) = state
        .running_map()
        .trades_desc()
        .next()
        .expect("has running trade");

    let cash_in = 15_000;

    executor
        .cash_in(trade.id(), cash_in.try_into().unwrap())
        .await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        start_balance - state.running_short_margin() - state.running_fees() + 10_204;

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950 - cash_in + 10_204);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 10_204);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close trade

    executor.close_trade(trade.id()).await?;

    let state = executor.trading_state().await?;

    let expected_balance = start_balance as i64 + state.closed_pl() - state.closed_fees() as i64;

    assert_eq!(state.last_tick_time(), time);
    assert!((state.balance() as i64 - expected_balance).abs() < 2);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_pl(), 10_204);
    assert_eq!(state.closed_fees(), 1_010);

    Ok(())
}

#[tokio::test]
async fn test_simulated_trade_executor_add_margin_short_loss() -> TradeExecutorResult<()> {
    // Step 1: Create a new executor with market price as 100_000, balance of 1_000_000

    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap();
    let max_running_qtd = 10;

    let executor = SimulatedTradeExecutor::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = executor.trading_state().await?;
    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), start_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade

    let size = Quantity::try_from(500).unwrap().into();
    let leverage = Leverage::try_from(10).unwrap();
    let stoploss_price = Price::clamp_from(102_000.);
    let stoploss = Some(Stoploss::fixed(stoploss_price));
    let takeprofit = Some(Price::clamp_from(98_000.));

    executor
        .open_short(size, leverage, stoploss, takeprofit)
        .await?;

    let state = executor.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), start_time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 500);
    assert_eq!(state.closed_len(), 0);

    // Step 3: Update price to 101_000 (1% increase)

    let time = start_time + Duration::seconds(1);
    let market_price = 101_000.0;
    executor.tick_update(time, market_price).await?;

    let state = executor.trading_state().await?;
    let (trade, _) = state
        .running_map()
        .trades_desc()
        .next()
        .expect("has running trade");

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950);
    assert_eq!(state.running_pl(), -4_951);
    assert_eq!(state.closed_len(), 0);

    // Step 4: Add margin

    let add_margin = 5_000;
    executor
        .add_margin(trade.id(), add_margin.try_into().unwrap())
        .await?;
    let state = executor.trading_state().await?;

    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.last_tick_time(), time);
    assert_eq!(state.balance(), expected_balance);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.running_short_margin(), 50_950 + add_margin);
    assert_eq!(state.running_pl(), -4_951);
    assert_eq!(state.closed_len(), 0);

    // Step 5: Close trade

    executor.close_trade(trade.id()).await?;

    let state = executor.trading_state().await?;

    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.last_tick_time(), time);
    assert!((state.balance() as i64 - expected_balance as i64).abs() < 2);
    assert_eq!(state.market_price().into_f64(), market_price);
    assert_eq!(state.last_trade_time(), Some(time));
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.closed_len(), 1);
    assert_eq!(state.closed_pl(), -4_951);
    assert_eq!(state.closed_fees(), 995);

    Ok(())
}
