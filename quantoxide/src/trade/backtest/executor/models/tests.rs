use super::*;

use chrono::Utc;

fn get_lnm_fee() -> BoundedPercentage {
    BoundedPercentage::try_from(0.1).unwrap()
}

#[test]
fn test_long_liquidation_calculation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(),
        Price::try_from(95_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.liquidation.into_f64(), 81_819.0);
    assert_eq!(trade.opening_fee, 11);
    assert_eq!(trade.closing_fee_reserved, 12);
}

#[test]
fn test_short_liquidation_calculation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(),
        Price::try_from(85_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.liquidation.into_f64(), 99_999.0);
    assert_eq!(trade.opening_fee, 11);
    assert_eq!(trade.closing_fee_reserved, 10);
}

#[test]
fn test_short_liquidation_calculation_max_price() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(1).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(),
        Price::try_from(85_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.liquidation, Price::MAX);
    assert_eq!(trade.opening_fee, 11);
    assert_eq!(trade.closing_fee_reserved, 0);
}

#[test]
fn test_long_stoploss_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();
    // From test_long_liquidation_calculation, we know liquidation is at 81,819.0

    // Test: Stoploss must be below entry price for long positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(), // Invalid: above entry price
        Price::try_from(100_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        Price::try_from(100_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be below liquidation price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(81_000.0).unwrap(), // Invalid: below liquidation
        Price::try_from(100_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid long stoploss (between liquidation and entry)
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(), // Valid: above liquidation, below entry
        Price::try_from(100_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_long_takeprofit_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();
    let valid_stoploss = Price::try_from(85_000.0).unwrap();

    // Test: Takeprofit must be above entry price for long positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(85_000.0).unwrap(), // Invalid: below entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid long takeprofit
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(95_000.0).unwrap(), // Valid: above entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_stoploss_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();
    // From test_short_liquidation_calculation, we know liquidation is at 99,999.0

    // Test: Stoploss must be above entry price for short positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(), // Invalid: below entry
        Price::try_from(80_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        Price::try_from(85_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be above liquidation price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(100_500.0).unwrap(), // Invalid: above liquidation
        Price::try_from(85_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid short stoploss (between entry and liquidation)
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(), // Valid: above entry, below liquidation
        Price::try_from(85_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_takeprofit_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(10.0).unwrap();
    // Using valid stoploss that's below liquidation price
    let valid_stoploss = Price::try_from(95_000.0).unwrap();

    // Test: Takeprofit must be below entry price for short positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(95_000.0).unwrap(), // Invalid: above entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid short takeprofit
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(85_000.0).unwrap(), // Valid: below entry
        quantity,
        leverage,
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_running_long_pl_calculation() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        fee,
    )
    .unwrap();

    let expected_pl = 1818;
    let expected_net_pl = 1780;

    assert_eq!(trade.pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_long_pl_loss() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(45_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(42_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        fee,
    )
    .unwrap();

    let expected_pl = -2223;
    let expected_net_pl = -2265;

    assert_eq!(trade.pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_short_pl_calculation() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(45_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(55_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        quantity,
        leverage,
        fee,
    )
    .unwrap();

    let expected_pl = 2222;
    let expected_net_pl = 2180;

    assert_eq!(trade.pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_short_pl_loss() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(60_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        quantity,
        leverage,
        fee,
    )
    .unwrap();

    let expected_pl = -1819;
    let expected_net_pl = -1857;

    assert_eq!(trade.pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_closed_long_pl_calculation() {
    // Create a closed long trade
    let entry_price = Price::try_from(50_000.0).unwrap();
    let close_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();

    let running_trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    let closed_trade = SimulatedTradeClosed::from_running(
        running_trade.as_ref(),
        Utc::now(),
        close_price,
        get_lnm_fee(),
    );

    let expected_pl = 1818;

    assert_eq!(closed_trade.pl(), expected_pl);
    assert_eq!(closed_trade.pl(), running_trade.pl(close_price));
    assert_eq!(
        closed_trade.net_pl(),
        expected_pl - closed_trade.opening_fee as i64 - closed_trade.closing_fee as i64
    );
}

#[test]
fn test_closed_short_pl_calculation() {
    // Create a closed short trade
    let entry_price = Price::try_from(50_000.0).unwrap();
    let close_price = Price::try_from(45_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();

    let running_trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        Utc::now(),
        entry_price,
        Price::try_from(55_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    let closed_trade = SimulatedTradeClosed::from_running(
        running_trade.as_ref(),
        Utc::now(),
        close_price,
        get_lnm_fee(),
    );

    let expected_pl = 2222;

    assert_eq!(closed_trade.pl(), expected_pl);
    assert_eq!(closed_trade.pl(), running_trade.pl(close_price));
    assert_eq!(
        closed_trade.net_pl(),
        expected_pl - closed_trade.opening_fee as i64 - closed_trade.closing_fee as i64
    );
}

#[test]
fn test_edge_case_min_quantity() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.pl(current_price), 181);
}

#[test]
fn test_edge_case_max_quantity() {
    // Test with maximum quantity (500_000)
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(50_500.0).unwrap();
    let quantity = Quantity::try_from(500_000).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(49_000.0).unwrap(),
        Price::try_from(55_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.pl(current_price), 9900990);
}

#[test]
fn test_edge_case_min_leverage() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(1.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    // Leverage doesn't directly affect PL calculation, but it's important
    // for testing that our trade construction works with min leverage
    // PL should be the same as other tests with same price movement

    assert_eq!(trade.pl(current_price), 1818);
}

#[test]
fn test_edge_case_max_leverage() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(100.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(49_800.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    // Again, leverage doesn't directly affect PL calculation

    assert_eq!(trade.pl(current_price), 1818);
}

#[test]
fn test_edge_case_small_prices() {
    let entry_price = Price::try_from(1.5).unwrap();
    let current_price = Price::try_from(2.0).unwrap();
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(1.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(1.0).unwrap(),
        Price::try_from(2.0).unwrap(),
        quantity,
        leverage,
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.pl(current_price), 16_666_666);
}

#[test]
fn test_no_price_movement() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = entry_price;
    let quantity = Quantity::try_from(10).unwrap();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        quantity,
        leverage,
        fee,
    )
    .unwrap();

    assert_eq!(trade.pl(current_price), 0);
    assert_eq!(trade.net_pl_est(fee, current_price), -40);
}
