use super::*;

use chrono::Duration;

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage};

#[tokio::test]
async fn test_simulated_trades_manager_long_profit() -> Result<()> {
    // Step 1: Create a new manager with market price as 99_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 99_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let max_running_qtd = 10;

    let manager = SimulatedTradeManager::new(
        max_running_qtd,
        fee_perc,
        start_time,
        market_price,
        start_balance,
    );

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Update market price to 100_000
    let time = start_time + Duration::seconds(1);
    let market_price = 100_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Open a long trade using 5% of balance;
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    manager
        .open_long(stoploss_perc, takeprofit_perc, balance_perc, leverage)
        .await?;

    let state = manager.state().await?;
    let exp_trade_time = time;
    let expected_balance =
        start_balance - state.running_long_margin() - state.running_maintenance_margin();

    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_qtd(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert!(
        state.running_fees() < state.running_maintenance_margin(),
        "Estimated fees should be smaller than maintenance margin"
    );
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Update price to 101_000
    let time = time + Duration::seconds(1);
    let market_price = 101_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_qtd(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert!(
        state.running_pl() > 0,
        "Long position should be profitable after price increase"
    );
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert!(
        state.running_fees() < state.running_maintenance_margin(),
        "Estimated fees should be smaller than maintenance margin"
    );
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 5: Close all running long trades
    manager.close_longs().await?;

    let state = manager.state().await?;
    let exp_trade_time = time;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(exp_trade_time));
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 1);
    assert!(
        state.closed_pl() > 0,
        "Should have positive PL after closing profitable long"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trades_manager_long_loss() -> Result<()> {
    // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let max_running_qtd = 10;

    let manager = SimulatedTradeManager::new(
        max_running_qtd,
        fee_perc,
        start_time,
        market_price,
        start_balance,
    );

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a long trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    manager
        .open_long(stoploss_perc, takeprofit_perc, balance_perc, leverage)
        .await?;

    let state = manager.state().await?;
    let expected_balance =
        start_balance - state.running_long_margin() - state.running_maintenance_margin();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_qtd(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert!(
        state.running_fees() < state.running_maintenance_margin(),
        "Estimated fees should be smaller than maintenance margin"
    );
    assert_eq!(state.closed_qtd(), 0);

    // Step 3: Update price to 99_000 (1% drop)
    let time = start_time + Duration::seconds(1);
    let market_price = 99_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_qtd(), 1);
    assert!(
        state.running_long_margin() > 0,
        "Long margin should be positive"
    );
    assert!(
        state.running_pl() < 0,
        "Long position should be at a loss after price decrease"
    );
    assert_eq!(state.closed_qtd(), 0);

    // Step 4: Update price to trigger stoploss (98_000, 2% drop from entry)
    let time = time + Duration::seconds(1);
    let market_price = 98_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_qtd(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 1);
    assert!(
        state.closed_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trades_manager_short_profit() -> Result<()> {
    // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let max_running_qtd = 10;

    let manager = SimulatedTradeManager::new(
        max_running_qtd,
        fee_perc,
        start_time,
        market_price,
        start_balance,
    );

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(3.0).unwrap(); // 3% stoploss
    let takeprofit_perc = BoundedPercentage::try_from(4.0).unwrap(); // 4% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    manager
        .open_short(stoploss_perc, takeprofit_perc, balance_perc, leverage)
        .await?;

    let state = manager.state().await?;
    let expected_balance =
        start_balance - state.running_short_margin() - state.running_maintenance_margin();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0); // No PL yet since price hasn't changed
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert_eq!(state.running_fees(), state.running_maintenance_margin()); // No liquidation price
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 3: Update price to 98_000 (2% drop)
    let time = start_time + Duration::seconds(1);
    let market_price = 98_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_qtd(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert!(
        state.running_pl() > 0,
        "Short position should be profitable after price decrease"
    );
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert_eq!(state.running_fees(), state.running_maintenance_margin()); // No liquidation price
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 4: Update price to trigger takeprofit (96_000, 4% drop from entry)
    let time = time + Duration::seconds(1);
    let market_price = 96_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_qtd(), 0); // Trade should be closed by takeprofit
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 1);
    assert!(
        state.closed_pl() > 0,
        "Should have positive PL after hitting takeprofit"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}

#[tokio::test]
async fn test_simulated_trades_manager_short_loss() -> Result<()> {
    // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
    let start_time = Utc::now();
    let market_price = 100_000.0;
    let start_balance = 1_000_000;
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
    let max_running_qtd = 10;

    let manager = SimulatedTradeManager::new(
        max_running_qtd,
        fee_perc,
        start_time,
        market_price,
        start_balance,
    );

    let state = manager.state().await?;
    assert_eq!(state.start_time(), start_time);
    assert_eq!(state.start_balance(), start_balance);
    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), start_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), None);
    assert_eq!(state.running_long_qtd(), 0);
    assert_eq!(state.running_long_margin(), 0);
    assert_eq!(state.running_short_qtd(), 0);
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 0);
    assert_eq!(state.closed_pl(), 0);
    assert_eq!(state.closed_fees(), 0);

    // Step 2: Open a short trade using 5% of balance
    let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
    let takeprofit_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
    let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
    let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

    manager
        .open_short(stoploss_perc, takeprofit_perc, balance_perc, leverage)
        .await?;

    let state = manager.state().await?;
    let expected_balance =
        start_balance - state.running_short_margin() - state.running_maintenance_margin();

    assert_eq!(state.current_time(), start_time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_qtd(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert_eq!(state.running_pl(), 0);
    assert!(state.running_fees() > 0, "Trading fees should be estimated");
    assert!(
        state.running_maintenance_margin() > 0,
        "Trading maintenance margin must be estimated"
    );
    assert_eq!(state.running_fees(), state.running_maintenance_margin()); // No liquidation price
    assert_eq!(state.closed_qtd(), 0);

    // Step 3: Update price to 101_000 (1% increase)
    let time = start_time + Duration::seconds(1);
    let market_price = 101_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_qtd(), 1);
    assert!(
        state.running_short_margin() > 0,
        "Short margin should be positive"
    );
    assert!(
        state.running_pl() < 0,
        "Short position should be at a loss after price increase"
    );
    assert_eq!(state.closed_qtd(), 0);

    // Step 4: Update price to trigger stoploss (102_000, 2% increase from entry)
    let time = time + Duration::seconds(1);
    let market_price = 102_000.0;
    manager.tick_update(time, market_price).await?;

    let state = manager.state().await?;
    let expected_balance =
        (start_balance as i64 + state.closed_pl() - state.closed_fees() as i64) as u64;

    assert_eq!(state.current_time(), time);
    assert_eq!(state.current_balance(), expected_balance);
    assert_eq!(state.market_price(), market_price);
    assert_eq!(state.last_trade_time(), Some(start_time));
    assert_eq!(state.running_short_qtd(), 0); // Trade should be closed by stoploss
    assert_eq!(state.running_short_margin(), 0);
    assert_eq!(state.running_pl(), 0);
    assert_eq!(state.running_fees(), 0);
    assert_eq!(state.running_maintenance_margin(), 0);
    assert_eq!(state.closed_qtd(), 1);
    assert!(
        state.closed_pl() < 0,
        "Should have negative PL after hitting stoploss"
    );
    assert!(state.closed_fees() > 0, "Should have paid trading fees");

    Ok(())
}
