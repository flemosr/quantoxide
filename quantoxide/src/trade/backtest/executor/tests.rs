use super::*;

use chrono::Duration;

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage};

#[tokio::test]
async fn test_simulated_trade_controller_long_profit() -> Result<()> {
    // Step 1: Create a new controller with market price as 99_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 99_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
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

    // Step 3: Open a long trade using 5% of balance;
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let stoploss_mode = StoplossMode::Fixed;
    let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    controller
        .open_long(
            stoploss_perc,
            stoploss_mode,
            takeprofit_perc,
            balance_perc,
            leverage,
        )
        .await?;

    let state = controller.trading_state().await?;
    let exp_trade_time = time;
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.close_longs().await?;

    let state = controller.trading_state().await?;
    let exp_trade_time = time;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
async fn test_simulated_trade_controller_long_loss() -> Result<()> {
    // Step 1: Create a new controller with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
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

    // Step 2: Open a long trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let stoploss_mode = StoplossMode::Fixed;
    let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    controller
        .open_long(
            stoploss_perc,
            stoploss_mode,
            takeprofit_perc,
            balance_perc,
            leverage,
        )
        .await?;

    let state = controller.trading_state().await?;
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
async fn test_simulated_trade_controller_short_profit() -> Result<()> {
    // Step 1: Create a new controller with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
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

    // Step 2: Open a short trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(3.0).unwrap(); // 3% stoploss
    let stoploss_mode = StoplossMode::Fixed;
    let takeprofit_perc = BoundedPercentage::try_from(4.0).unwrap(); // 4% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    controller
        .open_short(
            stoploss_perc,
            stoploss_mode,
            takeprofit_perc,
            balance_perc,
            leverage,
        )
        .await?;

    let state = controller.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
async fn test_simulated_trade_controller_short_loss() -> Result<()> {
    // Step 1: Create a new controller with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
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

    // Step 2: Open a short trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let stoploss_mode = StoplossMode::Fixed;
    let takeprofit_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    controller
        .open_short(
            stoploss_perc,
            stoploss_mode,
            takeprofit_perc,
            balance_perc,
            leverage,
        )
        .await?;

    let state = controller.trading_state().await?;
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
    controller.tick_update(time, market_price).await?;

    let state = controller.trading_state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
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
async fn test_simulated_trade_controller_trailing_stoploss_long() {
    let start_time = Utc::now();
    let start_balance = 100_000_000;
    let market_price = 100_000.0;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    // Open long position with trailing stop-loss
    controller
        .open_long(
            BoundedPercentage::try_from(2.0).unwrap(), // 2% trailing stop-loss
            StoplossMode::Trailing,
            LowerBoundedPercentage::try_from(4.0).unwrap(), // 4% take-profit
            BoundedPercentage::try_from(10.0).unwrap(),     // 10% of balance
            Leverage::try_from(1).unwrap(),
        )
        .await
        .unwrap();

    let state = controller.trading_state().await.unwrap();
    let expected_balance = start_balance - state.running_long_margin() - state.running_fees();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.running_long_len(), 1);
    assert_eq!(state.running_short_len(), 0);
    assert_eq!(state.closed_len(), 0);

    // Price increases to 102_000 (2% increase)
    // Trailing stoploss should move from 98_000 to 99_960 (2% below 102_000)
    let time = start_time + chrono::Duration::seconds(1);
    controller.tick_update(time, 102_000.0).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_long_len(), 1); // Position still open

    // Price drops to 99_960.5
    // Should still be above new stop-loss (99_960)
    let time = time + chrono::Duration::seconds(1);
    controller.tick_update(time, 99_960.5).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_long_len(), 1); // Position still open

    // Price drops to 99_960
    // Should trigger the trailing stop-loss (99_960)
    let time = time + chrono::Duration::seconds(1);
    controller.tick_update(time, 99_960.0).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_long_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}

#[tokio::test]
async fn test_simulated_trade_controller_trailing_stoploss_short() {
    let start_time = Utc::now();
    let start_balance = 100_000_000;
    let market_price = 100_000.0;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let tsl_step_size = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% trailing stop loss step size
    let max_running_qtd = 10;

    let controller = SimulatedTradeController::new(
        max_running_qtd,
        fee_perc,
        tsl_step_size,
        start_time,
        market_price,
        start_balance,
    );

    // Open short position with trailing stop-loss
    controller
        .open_short(
            BoundedPercentage::try_from(2.0).unwrap(), // 2% trailing stop-loss
            StoplossMode::Trailing,
            BoundedPercentage::try_from(4.0).unwrap(), // 4% take-profit
            BoundedPercentage::try_from(10.0).unwrap(), // 10% of balance
            Leverage::try_from(1).unwrap(),
        )
        .await
        .unwrap();

    let state = controller.trading_state().await.unwrap();
    let expected_balance = start_balance - state.running_short_margin() - state.running_fees();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.running_long_len(), 0);
    assert_eq!(state.running_short_len(), 1);
    assert_eq!(state.closed_len(), 0);

    // Price decreases to 98_000 (2% decrease)
    // Trailing stoploss should move from 102_000 to 99_960 (2% above 98_000)
    let time = start_time + chrono::Duration::seconds(1);
    controller.tick_update(time, 98_000.0).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_short_len(), 1); // Position still open

    // Price increases to 99_959.5
    // Should still be below new stop-loss (99_960)
    let time = time + chrono::Duration::seconds(1);
    controller.tick_update(time, 99_959.5).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_short_len(), 1); // Position still open

    // Price increases to 99_960
    // Should trigger the trailing stop-loss (99_960)
    let time = time + chrono::Duration::seconds(1);
    controller.tick_update(time, 99_960.0).await.unwrap();

    let state = controller.trading_state().await.unwrap();
    assert_eq!(state.running_short_len(), 0); // Position closed
    assert_eq!(state.closed_len(), 1);
}
