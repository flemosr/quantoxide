use super::*;

use chrono::Utc;

fn get_lnm_fee() -> BoundedPercentage {
    BoundedPercentage::try_from(0.1).unwrap()
}

#[test]
fn test_long_liquidation_calculation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(),
        Price::try_from(95_000.0).unwrap(),
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
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(),
        Price::try_from(85_000.0).unwrap(),
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
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(),
        Price::try_from(85_000.0).unwrap(),
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
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    // From test_long_liquidation_calculation, we know liquidation is at 81,819.0

    // Test: Stoploss must be below entry price for long positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(), // Invalid: above entry price
        Price::try_from(100_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        Price::try_from(100_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be below liquidation price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(81_000.0).unwrap(), // Invalid: below liquidation
        Price::try_from(100_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid long stoploss (between liquidation and entry)
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(), // Valid: above liquidation, below entry
        Price::try_from(100_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_long_takeprofit_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let valid_stoploss = Price::try_from(85_000.0).unwrap();

    // Test: Takeprofit must be above entry price for long positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(85_000.0).unwrap(), // Invalid: below entry
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid long takeprofit
    let result = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(95_000.0).unwrap(), // Valid: above entry
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_stoploss_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    // From test_short_liquidation_calculation, we know liquidation is at 99,999.0

    // Test: Stoploss must be above entry price for short positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(85_000.0).unwrap(), // Invalid: below entry
        Price::try_from(80_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        Price::try_from(85_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be above liquidation price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(100_500.0).unwrap(), // Invalid: above liquidation
        Price::try_from(85_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid short stoploss (between entry and liquidation)
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(95_000.0).unwrap(), // Valid: above entry, below liquidation
        Price::try_from(85_000.0).unwrap(),
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_takeprofit_validation() {
    let entry_price = Price::try_from(90_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    // Using valid stoploss that's below liquidation price
    let valid_stoploss = Price::try_from(95_000.0).unwrap();

    // Test: Takeprofit must be below entry price for short positions
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(95_000.0).unwrap(), // Invalid: above entry
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
        get_lnm_fee(),
    );
    assert!(result.is_err());

    // Test: Valid short takeprofit
    let result = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        valid_stoploss,
        Price::try_from(85_000.0).unwrap(), // Valid: below entry
        get_lnm_fee(),
    );
    assert!(result.is_ok());
}

#[test]
fn test_running_long_pl_calculation() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        fee,
    )
    .unwrap();

    let expected_pl = 1818;
    let expected_net_pl = 1780;

    assert_eq!(trade.est_pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_long_pl_loss() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(45_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(42_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        fee,
    )
    .unwrap();

    let expected_pl = -2223;
    let expected_net_pl = -2265;

    assert_eq!(trade.est_pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_short_pl_calculation() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(45_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(55_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        fee,
    )
    .unwrap();

    let expected_pl = 2222;
    let expected_net_pl = 2180;

    assert_eq!(trade.est_pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_running_short_pl_loss() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(60_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        fee,
    )
    .unwrap();

    let expected_pl = -1819;
    let expected_net_pl = -1857;

    assert_eq!(trade.est_pl(current_price), expected_pl);
    assert_eq!(trade.net_pl_est(fee, current_price), expected_net_pl);
}

#[test]
fn test_closed_long_pl_calculation() {
    // Create a closed long trade
    let entry_price = Price::try_from(50_000.0).unwrap();
    let close_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();

    let running_trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    let closed_trade = running_trade.to_closed(Utc::now(), close_price, get_lnm_fee());

    let expected_pl = 1818;

    assert_eq!(closed_trade.pl(), expected_pl);
    assert_eq!(closed_trade.pl(), running_trade.est_pl(close_price));
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
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();

    let running_trade = SimulatedTradeRunning::new(
        TradeSide::Sell,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(55_000.0).unwrap(),
        Price::try_from(45_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    let closed_trade = running_trade.to_closed(Utc::now(), close_price, get_lnm_fee());

    let expected_pl = 2222;

    assert_eq!(closed_trade.pl(), expected_pl);
    assert_eq!(closed_trade.pl(), running_trade.est_pl(close_price));
    assert_eq!(
        closed_trade.net_pl(),
        expected_pl - closed_trade.opening_fee as i64 - closed_trade.closing_fee as i64
    );
}

#[test]
fn test_edge_case_min_quantity() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(1).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.est_pl(current_price), 181);
}

#[test]
fn test_edge_case_max_quantity() {
    // Test with maximum quantity (500_000)
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(50_500.0).unwrap();
    let size = Quantity::try_from(500_000).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(49_000.0).unwrap(),
        Price::try_from(55_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.est_pl(current_price), 9900990);
}

#[test]
fn test_edge_case_min_leverage() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(1.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    // Leverage doesn't directly affect PL calculation, but it's important
    // for testing that our trade construction works with min leverage
    // PL should be the same as other tests with same price movement

    assert_eq!(trade.est_pl(current_price), 1818);
}

#[test]
fn test_edge_case_max_leverage() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = Price::try_from(55_000.0).unwrap();
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(100.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(49_800.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    // Again, leverage doesn't directly affect PL calculation

    assert_eq!(trade.est_pl(current_price), 1818);
}

#[test]
fn test_edge_case_small_prices() {
    let entry_price = Price::try_from(1.5).unwrap();
    let current_price = Price::try_from(2.0).unwrap();
    let size = Quantity::try_from(1).unwrap().into();
    let leverage = Leverage::try_from(1.0).unwrap();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(1.0).unwrap(),
        Price::try_from(2.0).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    assert_eq!(trade.est_pl(current_price), 16_666_666);
}

#[test]
fn test_no_price_movement() {
    let entry_price = Price::try_from(50_000.0).unwrap();
    let current_price = entry_price;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(5.0).unwrap();
    let fee = get_lnm_fee();

    let trade = SimulatedTradeRunning::new(
        TradeSide::Buy,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(45_000.0).unwrap(),
        Price::try_from(60_000.0).unwrap(),
        fee,
    )
    .unwrap();

    assert_eq!(trade.est_pl(current_price), 0);
    assert_eq!(trade.net_pl_est(fee, current_price), -40);
}

#[test]
fn test_add_margin_long_position() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2.0).unwrap();
    let entry_time = Utc::now();
    let entry_price = Price::try_from(50_000.0).unwrap();
    let stoploss = Price::try_from(40_000.0).unwrap();
    let takeprofit = Price::try_from(60_000.0).unwrap();
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        entry_time,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    let additional_margin = NonZeroU64::new(50_000).unwrap();
    let updated_trade = original_trade.with_added_margin(additional_margin).unwrap();

    assert_eq!(
        updated_trade.margin(),
        original_trade.margin() + additional_margin.into()
    );

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    assert!(updated_trade.leverage() < original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert!(updated_trade.liquidation() < original_trade.liquidation());
    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Verify other fields remain unchanged
    assert_eq!(updated_trade.id(), original_trade.id());
    assert_eq!(updated_trade.quantity(), original_trade.quantity());
    assert_eq!(updated_trade.entry_price(), original_trade.entry_price());
    assert_eq!(updated_trade.price(), original_trade.price());
    assert_eq!(updated_trade.stoploss(), original_trade.stoploss());
    assert_eq!(updated_trade.takeprofit(), original_trade.takeprofit());
}

#[test]
fn test_add_margin_short_position() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2.0).unwrap();
    let entry_time = Utc::now();
    let entry_price = Price::try_from(50_000.0).unwrap();
    let stoploss = Price::try_from(60_000.0).unwrap();
    let takeprofit = Price::try_from(40_000.0).unwrap();
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        entry_time,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    let additional_margin = NonZeroU64::new(50_000).unwrap();
    let updated_trade = original_trade.with_added_margin(additional_margin).unwrap();

    assert_eq!(
        updated_trade.margin(),
        original_trade.margin() + additional_margin.into()
    );

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    assert!(updated_trade.leverage() < original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert!(updated_trade.liquidation() > original_trade.liquidation());
    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Verify other fields remain unchanged
    assert_eq!(updated_trade.id(), original_trade.id());
    assert_eq!(updated_trade.quantity(), original_trade.quantity());
    assert_eq!(updated_trade.entry_price(), original_trade.entry_price());
    assert_eq!(updated_trade.price(), original_trade.price());
    assert_eq!(updated_trade.stoploss(), original_trade.stoploss());
    assert_eq!(updated_trade.takeprofit(), original_trade.takeprofit());
}

#[test]
fn test_cash_in_from_profitable_long_trade() {
    let entry_price = Price::try_from(95_238.0).unwrap();

    let side = TradeSide::Buy;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        Utc::now(),
        entry_price,
        Price::try_from(90_000).unwrap(),
        Price::try_from(110_000).unwrap(),
        get_lnm_fee(),
    )
    .unwrap();

    let market_price = Price::try_from(100_000).unwrap();
    let initial_pl = original_trade.est_pl(market_price);
    assert_eq!(initial_pl, 50_001);

    // Case 1: Cash in partial profit

    let cash_in_amount = NonZeroU64::new(10_001).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    // After cashing in 10_001, the new entry price should be adjusted
    // so that remaining PL is aprox 40_000 at the current price

    assert!((updated_trade.est_pl(market_price) - 40_000 as i64).abs() < 5);
    assert_eq!(updated_trade.margin(), original_trade.margin());

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should remain aprox the same
    assert!(
        (updated_trade.leverage().into_f64() - original_trade.leverage().into_f64()).abs() < 0.5
    );
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Case 2: Cash in all profit

    let cash_in_amount = NonZeroU64::new(50_001).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    assert_eq!(updated_trade.price, market_price);
    assert_eq!(updated_trade.est_pl(market_price), 0);
    assert_eq!(updated_trade.margin(), original_trade.margin());

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should remain aprox the same
    assert!(
        (updated_trade.leverage().into_f64() - original_trade.leverage().into_f64()).abs() < 0.5
    );
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Case 3: Cash in more than profit

    let cash_in_amount = NonZeroU64::new(100_001).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();
    let expected_remaining_margin =
        original_trade.margin().into_u64() + initial_pl as u64 - cash_in_amount.get() as u64;

    assert_eq!(updated_trade.price, market_price);
    assert_eq!(updated_trade.est_pl(market_price), 0);
    assert_eq!(updated_trade.margin().into_u64(), expected_remaining_margin);

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should increase
    assert!(updated_trade.leverage() > original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);
}

#[test]
fn test_cash_in_from_losing_long_trade() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2.0).unwrap();
    let entry_time = Utc::now();
    let entry_price = Price::try_from(50_000.0).unwrap();
    let stoploss = Price::try_from(40_000.0).unwrap();
    let takeprofit = Price::try_from(60_000.0).unwrap();
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        entry_time,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    let market_price = Price::try_from(45_000.0).unwrap();
    let current_pl = original_trade.est_pl(market_price);
    assert!(current_pl < 0);

    let original_margin = original_trade.margin();

    let cash_in_amount = NonZeroU64::new(30_000).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    // Cash-in amount must come from margin
    assert_eq!(
        updated_trade.margin().into_u64(),
        original_margin.into_u64() - 30_000
    );

    // Trade price should remain unchanged
    assert_eq!(updated_trade.price(), original_trade.price());

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should increase
    assert!(updated_trade.leverage() > original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // PL estimate should remain the same relative to market price
    assert_eq!(updated_trade.est_pl(market_price), current_pl);
}

#[test]
fn test_cash_in_from_profitable_short_trade() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2.0).unwrap();
    let entry_time = Utc::now();
    let entry_price = Price::try_from(50_000.0).unwrap();
    let stoploss = Price::try_from(60_000.0).unwrap();
    let takeprofit = Price::try_from(40_000.0).unwrap();
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        entry_time,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    let market_price = Price::try_from(45_000.0).unwrap();

    // Verify initial PL
    let initial_pl = original_trade.est_pl(market_price);
    assert_eq!(initial_pl, 222_222);

    // Case 1: Cash in partial profit

    let cash_in_amount = NonZeroU64::new(22_222).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    // After cashing in 22_222, the new entry price should be adjusted
    // so that remaining PL is aprox 200_000 at the current price

    assert!((updated_trade.est_pl(market_price) - 200_000 as i64).abs() < 5);
    assert_eq!(updated_trade.margin(), original_trade.margin()); // Margin should remain unchanged

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should remain aprox the same
    assert!(
        (updated_trade.leverage().into_f64() - original_trade.leverage().into_f64()).abs() < 0.5
    );
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Case 2: Cash in all profit

    let cash_in_amount = NonZeroU64::new(222_222).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    assert_eq!(updated_trade.price, market_price);
    assert_eq!(updated_trade.est_pl(market_price), 0);
    assert_eq!(updated_trade.margin(), original_trade.margin()); // Margin should remain unchanged

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should remain aprox the same
    assert!(
        (updated_trade.leverage().into_f64() - original_trade.leverage().into_f64()).abs() < 0.5
    );
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // Case 3: Cash in more than profit

    let cash_in_amount = NonZeroU64::new(300_000).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();
    let expected_remaining_margin =
        original_trade.margin().into_u64() + initial_pl as u64 - cash_in_amount.get() as u64;

    assert_eq!(updated_trade.price, market_price);
    assert_eq!(updated_trade.est_pl(market_price), 0);
    assert_eq!(updated_trade.margin().into_u64(), expected_remaining_margin);

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should increase
    assert!(updated_trade.leverage() > original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);
}

#[test]
fn test_cash_in_from_losing_short_trade() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(1_000).unwrap().into();
    let leverage = Leverage::try_from(2.0).unwrap();
    let entry_time = Utc::now();
    let entry_price = Price::try_from(50_000.0).unwrap();
    let stoploss = Price::try_from(60_000.0).unwrap();
    let takeprofit = Price::try_from(40_000.0).unwrap();
    let fee_perc = BoundedPercentage::try_from(0.1).unwrap();

    let original_trade = SimulatedTradeRunning::new(
        side,
        size,
        leverage,
        entry_time,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    // Market price rises to 55,000 (losing position for short)
    let market_price = Price::try_from(55_000.0).unwrap();
    let current_pl = original_trade.est_pl(market_price);
    assert!(current_pl < 0);

    let original_margin = original_trade.margin();

    // Try to cash in 25,000 sats when in a losing position
    let cash_in_amount = NonZeroU64::new(25_000).unwrap();
    let updated_trade = original_trade
        .with_cash_in(market_price, cash_in_amount)
        .unwrap();

    // Since there's no profit to cash in, the amount should come from margin
    assert_eq!(
        updated_trade.margin().into_u64(),
        original_margin.into_u64() - 25_000
    );

    // Trade price should remain unchanged (no PL to cash in)
    assert_eq!(updated_trade.price(), original_trade.price());

    let expected_leverage = Leverage::try_calculate(
        updated_trade.quantity(),
        updated_trade.margin(),
        updated_trade.price(),
    )
    .unwrap();

    // Leverage should increase
    assert!(updated_trade.leverage() > original_trade.leverage());
    assert_eq!(updated_trade.leverage(), expected_leverage);

    let expected_liquidation = trade_util::estimate_liquidation_price(
        side,
        updated_trade.quantity(),
        updated_trade.price(),
        updated_trade.leverage(),
    );

    assert_eq!(updated_trade.liquidation(), expected_liquidation);

    // PL estimate should remain the same relative to market price
    assert_eq!(updated_trade.est_pl(market_price), current_pl);
}
