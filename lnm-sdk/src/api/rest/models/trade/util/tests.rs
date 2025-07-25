use super::*;

#[test]
fn test_estimate_liquidation_price() {
    // Test case 1: Buy side with min leverage

    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let entry_price = Price::try_from(110_000).unwrap();
    let leverage = Leverage::MIN;

    let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
    let expected_liquidation_price = Price::try_from(55_000).unwrap();

    assert_eq!(liquidation_price, expected_liquidation_price);

    // Test case 2: Buy side with max leverage

    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let entry_price = Price::try_from(110_000).unwrap();
    let leverage = Leverage::MAX;

    let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
    let expected_liquidation_price = Price::try_from(108_911).unwrap();

    assert_eq!(liquidation_price, expected_liquidation_price);

    // Test case 3: Sell side with min leverage

    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let entry_price = Price::try_from(110_000).unwrap();
    let leverage = Leverage::MIN;

    let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
    let expected_liquidation_price = Price::MAX;

    assert_eq!(liquidation_price, expected_liquidation_price);

    // Test case 4: Sell side with max leverage

    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let entry_price = Price::try_from(110_000).unwrap();
    let leverage = Leverage::MAX;

    let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
    let expected_liquidation_price = Price::try_from(111_111).unwrap();

    assert_eq!(liquidation_price, expected_liquidation_price);
}

fn get_lnm_fee() -> BoundedPercentage {
    BoundedPercentage::try_from(0.1).unwrap()
}

#[test]
fn test_long_liquidation_calculation() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let stoploss = Price::try_from(85_000.0).unwrap();
    let takeprofit = Price::try_from(95_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    let (_, _, liquidation, opening_fee, closing_fee_reserved) = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    assert_eq!(liquidation.into_f64(), 81_819.0);
    assert_eq!(opening_fee, 11);
    assert_eq!(closing_fee_reserved, 12);
}

#[test]
fn test_short_liquidation_calculation() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let stoploss = Price::try_from(95_000.0).unwrap();
    let takeprofit = Price::try_from(85_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    let (_, _, liquidation, opening_fee, closing_fee_reserved) = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    assert_eq!(liquidation.into_f64(), 99_999.0);
    assert_eq!(opening_fee, 11);
    assert_eq!(closing_fee_reserved, 10);
}

#[test]
fn test_short_liquidation_calculation_max_price() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(1).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let stoploss = Price::try_from(95_000.0).unwrap();
    let takeprofit = Price::try_from(85_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    let (_, _, liquidation, opening_fee, closing_fee_reserved) = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    )
    .unwrap();

    assert_eq!(liquidation, Price::MAX);
    assert_eq!(opening_fee, 11);
    assert_eq!(closing_fee_reserved, 0);
}

#[test]
fn test_long_stoploss_validation() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let takeprofit = Price::try_from(100_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    // Test: Stoploss must be below entry price for long positions

    let stoploss = Price::try_from(95_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price

    let stoploss = Price::try_from(90_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be below liquidation price
    // From `test_long_liquidation_calculation`, liquidation is at 81,819.0

    let stoploss = Price::try_from(81_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Valid long stoploss (between liquidation and entry)

    let stoploss = Price::try_from(85_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_ok());
}

#[test]
fn test_long_takeprofit_validation() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let stoploss = Price::try_from(85_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    // Test: Takeprofit must be above entry price for long positions

    let takeprofit = Price::try_from(85_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price

    let takeprofit = Price::try_from(90_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Valid long takeprofit (above entry price)

    let takeprofit = Price::try_from(95_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_stoploss_validation() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let takeprofit = Price::try_from(85_000.0).unwrap();
    let fee_perc = get_lnm_fee();

    // Test: Stoploss must be above entry price for short positions

    let stoploss = Price::try_from(85_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Stoploss cannot be equal to entry price

    let stoploss = Price::try_from(90_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Valid short stoploss (above entry price)

    let stoploss = Price::try_from(95_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_ok());
}

#[test]
fn test_short_takeprofit_validation() {
    let side = TradeSide::Sell;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let stoploss = Price::try_from(95_000.0).unwrap(); // Valid stoploss above entry
    let fee_perc = get_lnm_fee();

    // Test: Takeprofit must be below entry price for short positions

    let takeprofit = Price::try_from(95_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Takeprofit cannot be equal to entry price

    let takeprofit = Price::try_from(90_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_err());

    // Test: Valid short takeprofit (below entry price)

    let takeprofit = Price::try_from(85_000.0).unwrap();

    let result = evaluate_open_trade_params(
        side,
        size,
        leverage,
        entry_price,
        stoploss,
        takeprofit,
        fee_perc,
    );
    assert!(result.is_ok());
}

#[test]
fn test_pl_long_profit() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(10).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(55_000.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 1818);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_long_loss() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(10).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(45_000.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -2223);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_short_profit() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(10).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(45_000.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 2222);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_short_loss() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(10).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(55_000.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -1819);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_no_price_movement() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(10).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = start_price; // No price movement

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 0);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_edge_case_small_prices() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let start_price = Price::try_from(1.0).unwrap();
    let end_price = Price::try_from(1.5).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 33_333_333);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_edge_case_min_quantity() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(55_000.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 181);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_edge_case_max_quantity() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(500_000).unwrap();
    let start_price = Price::try_from(50_000.0).unwrap();
    let end_price = Price::try_from(50_500.0).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 9900990);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}
