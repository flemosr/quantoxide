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
fn test_long_stoploss_validation() {
    let side = TradeSide::Buy;
    let size = Quantity::try_from(10).unwrap().into();
    let leverage = Leverage::try_from(10.0).unwrap();
    let entry_price = Price::try_from(90_000.0).unwrap();
    let takeprofit = None;
    let fee_perc = get_lnm_fee();

    // Test: Stoploss must be below entry price for long positions

    let stoploss = Some(Price::try_from(95_000.0).unwrap());

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

    let stoploss = Some(Price::try_from(90_000.0).unwrap());

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

    let stoploss = Some(Price::try_from(81_000.0).unwrap());

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

    let stoploss = Some(Price::try_from(85_000.0).unwrap());

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
    let stoploss = None;
    let fee_perc = get_lnm_fee();

    // Test: Takeprofit must be above entry price for long positions

    let takeprofit = Some(Price::try_from(85_000.0).unwrap());

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

    let takeprofit = Some(Price::try_from(90_000.0).unwrap());

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

    let takeprofit = Some(Price::try_from(95_000.0).unwrap());

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
    let takeprofit = None;
    let fee_perc = get_lnm_fee();

    // Test: Stoploss must be above entry price for short positions

    let stoploss = Some(Price::try_from(85_000.0).unwrap());

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

    let stoploss = Some(Price::try_from(90_000.0).unwrap());

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

    let stoploss = Some(Price::try_from(95_000.0).unwrap());

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
    let stoploss = None;
    let fee_perc = get_lnm_fee();

    // Test: Takeprofit must be below entry price for short positions

    let takeprofit = Some(Price::try_from(95_000.0).unwrap());

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

    let takeprofit = Some(Price::try_from(90_000.0).unwrap());

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

    let takeprofit = Some(Price::try_from(85_000.0).unwrap());

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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 1818.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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
    let end_price = Price::try_from(46_000.0).unwrap();

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -1740.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 2222.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -1819.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 0.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 33_333_333.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_pl_edge_case_big_prices() {
    let side = TradeSide::Buy;
    let quantity = Quantity::MAX;
    let start_price = Price::try_from(95_000_000).unwrap();
    let end_price = Price::try_from(100_000_000.).unwrap();

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 26_315.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 181.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.0005; // 0.05%, higher tolerance needed
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

    let pl = estimate_pl(side, quantity, start_price, end_price).floor();
    let calculated_end_price = estimate_price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 9900990.);
    let price_diff = (calculated_end_price.into_f64() - end_price.into_f64()).abs();
    let tolerance = end_price.into_f64() * 0.00005; // 0.005%
    assert!(
        price_diff < tolerance,
        "Price difference {price_diff} exceeds tolerance",
    );
}

#[test]
fn test_added_margin_long_position() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let price = Price::try_from(50_000.0).unwrap();
    let original_leverage = Leverage::try_from(50.).unwrap();
    let original_margin = Margin::calculate(quantity, price, original_leverage);
    let additional_margin = NonZeroU64::new(50_000).unwrap();

    let (new_margin, new_leverage, new_liquidation) =
        evaluate_added_margin(side, quantity, price, original_margin, additional_margin).unwrap();

    assert_eq!(new_margin, original_margin + additional_margin.into());
    assert!(new_leverage < original_leverage);

    let original_liquidation = estimate_liquidation_price(side, quantity, price, original_leverage);
    assert!(new_liquidation < original_liquidation);
}

#[test]
fn test_added_margin_short_position() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let price = Price::try_from(50_000.0).unwrap();
    let original_leverage = Leverage::try_from(50.).unwrap();
    let original_margin = Margin::calculate(quantity, price, original_leverage);
    let additional_margin = NonZeroU64::new(50_000).unwrap();

    let (new_margin, new_leverage, new_liquidation) =
        evaluate_added_margin(side, quantity, price, original_margin, additional_margin).unwrap();

    assert_eq!(new_margin, original_margin + additional_margin.into());
    assert!(new_leverage < original_leverage);

    let original_liquidation = estimate_liquidation_price(side, quantity, price, original_leverage);
    assert!(new_liquidation > original_liquidation);
}

#[test]
fn test_cash_in_from_long_profit() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 90_909.0);

    let original_stoploss = Some(Price::try_from(95_000.0).unwrap());
    assert!(original_stoploss.unwrap() > original_liquidation);
    assert!(original_stoploss.unwrap() < original_price);

    let market_price = Price::try_from(110_000).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, 90_909.);

    // Case 1: Cash in partial profit

    let cash_in_amount = NonZeroU64::new(40_000).unwrap();
    assert!(cash_in_amount.get() < original_pl as u64);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert!(new_price > original_price);
    assert!(new_price < market_price);
    assert_eq!(new_price.into_f64(), 104_166.5);

    // New price should be adjusted so that remaining PL is aprox `cash_in_amount` at `market_price`

    let expected_updated_pl = original_pl - cash_in_amount.get() as f64;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price);
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    assert_eq!(new_margin, original_margin);

    assert_eq!(new_leverage.into_f64(), 9.600015360024576);

    assert!(new_liquidation > original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 94339.5);

    // Stoploss still above liquidation, should remain unchanged
    assert_eq!(new_stoploss, original_stoploss);

    // Case 2: Cash in all profit

    let cash_in_amount = NonZeroU64::new(90_909).unwrap();
    assert_eq!(cash_in_amount.get() as f64, original_pl);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert!(new_price > original_price);
    assert_eq!(new_price, market_price);

    let expected_updated_pl = 0.;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price);
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    assert_eq!(new_margin, original_margin);

    assert_eq!(new_leverage.into_f64(), 9.090909090909092);

    assert!(new_liquidation > original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 99099.0);

    // Stoploss was below new liquidation
    assert!(new_stoploss.is_none());

    // Case 3: Cash in more than profit

    let cash_in_amount = NonZeroU64::new(150_000).unwrap();
    assert!(cash_in_amount.get() as f64 > original_pl);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert!(new_price > original_price);
    assert_eq!(new_price, market_price);

    let expected_updated_pl = 0.;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price);
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    let expected_new_margin =
        original_margin.into_u64() + original_pl as u64 - cash_in_amount.get();
    assert_eq!(new_margin.into_u64(), expected_new_margin);

    assert!(new_leverage > original_leverage);
    assert_eq!(new_leverage.into_f64(), 22.22227160504801);

    assert!(new_liquidation > original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 105_263.0);

    // Stoploss was below new liquidation
    assert!(new_stoploss.is_none());
}

#[test]
fn test_cash_in_from_long_loss() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 90_909.0);

    let original_stoploss = Some(Price::try_from(95_000.0).unwrap());
    assert!(original_stoploss.unwrap() > original_liquidation);
    assert!(original_stoploss.unwrap() < original_price);

    let market_price = Price::try_from(98_000.0).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, -20_409.);

    let cash_in_amount = NonZeroU64::new(40_000).unwrap();

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert_eq!(new_price, original_price);

    let updated_pl = estimate_pl(side, quantity, new_price, market_price).floor();
    assert_eq!(updated_pl, original_pl);

    let expected_margin = original_margin.into_u64() - cash_in_amount.get();
    assert_eq!(new_margin.into_u64(), expected_margin);

    assert!(new_leverage > original_leverage);
    assert_eq!(new_leverage.into_f64(), 16.666666666666668);

    assert!(new_liquidation > original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 94339.5);

    // Stoploss still above liquidation, should remain unchanged
    assert_eq!(new_stoploss, original_stoploss);
}

#[test]
fn test_cash_in_from_short_profit() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 111_111.0);

    let original_stoploss = Some(Price::try_from(105_000.0).unwrap());
    assert!(original_stoploss.unwrap() < original_liquidation);
    assert!(original_stoploss.unwrap() > original_price);

    let market_price = Price::try_from(92_000).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, 86_956.);

    // Case 1: Cash in partial profit

    let cash_in_amount = NonZeroU64::new(30_000).unwrap();
    assert!(cash_in_amount.get() < original_pl as u64);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert!(new_price < original_price);
    assert!(new_price > market_price);
    assert_eq!(new_price.into_f64(), 97_087.5);

    // New price should be adjusted so that remaining PL is aprox `cash_in_amount` at `market_price`

    let expected_updated_pl = original_pl - cash_in_amount.get() as f64;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price);
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    assert_eq!(new_margin, original_margin);

    assert_eq!(new_leverage.into_f64(), 10.299987125016093);

    assert!(new_liquidation < original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 107_527.0);

    // Stoploss still below liquidation, should remain unchanged
    assert_eq!(new_stoploss, original_stoploss);

    // Case 2: Cash in all profit

    let cash_in_amount = NonZeroU64::new(86_956).unwrap();
    assert_eq!(cash_in_amount.get() as f64, original_pl);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert!(new_price < original_price);
    assert_eq!(new_price, market_price);

    let expected_updated_pl = 0.;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price).floor();
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    assert_eq!(new_margin, original_margin);

    assert_eq!(new_leverage.into_f64(), 10.869565217391305);

    assert!(new_liquidation < original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 101_321.5);

    // Stoploss was above new liquidation
    assert!(new_stoploss.is_none());

    // Case 3: Cash in more than profit

    let cash_in_amount = NonZeroU64::new(150_000).unwrap();
    assert!(cash_in_amount.get() as f64 > original_pl);

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    // assert!(new_price > original_price);
    assert_eq!(new_price, market_price);

    let expected_updated_pl = 0.;
    let updated_pl = estimate_pl(side, quantity, new_price, market_price).floor();
    assert!((updated_pl - expected_updated_pl).abs() < 5.);

    let expected_new_margin =
        original_margin.into_u64() + original_pl as u64 - cash_in_amount.get();
    assert_eq!(new_margin.into_u64(), expected_new_margin);

    assert!(new_leverage > original_leverage);
    assert_eq!(new_leverage.into_f64(), 29.412179936657928);

    assert!(new_liquidation < original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 95_238.0);

    // Stoploss was above new liquidation
    assert!(new_stoploss.is_none());
}

#[test]
fn test_cash_in_from_short_loss() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 111_111.0);

    let original_stoploss = Some(Price::try_from(105_000.0).unwrap());
    assert!(original_stoploss.unwrap() < original_liquidation);
    assert!(original_stoploss.unwrap() > original_price);

    let market_price = Price::try_from(102_000).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, -19_608.);

    let cash_in_amount = NonZeroU64::new(40_000).unwrap();

    let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        original_stoploss,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert_eq!(new_price, original_price);

    let updated_pl = estimate_pl(side, quantity, new_price, market_price).floor();
    assert_eq!(updated_pl, original_pl);

    let expected_margin = original_margin.into_u64() - cash_in_amount.get();
    assert_eq!(new_margin.into_u64(), expected_margin);

    assert!(new_leverage > original_leverage);
    assert_eq!(new_leverage.into_f64(), 16.666666666666668);

    assert!(new_liquidation < original_liquidation);
    assert_eq!(new_liquidation.into_f64(), 106_383.0);

    // Stoploss still below liquidation, should remain unchanged
    assert_eq!(new_stoploss, original_stoploss);
}

#[test]
fn test_collateral_delta_estimation_long_profit_leverage_up() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.0).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let market_price = Price::try_from(110_000.0).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, 90_909.);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 90909.0);

    // Case 1: Cash-in less than PL

    let target_liquidation = Price::try_from(95_000.0).unwrap();
    assert!(target_liquidation > original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, -47_368);
    assert!(collateral_delta.abs() < original_pl as i64);

    let cash_in_amount = NonZeroU64::new(collateral_delta.abs() as u64).unwrap();

    let (_, _, _, new_liquidation, _) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        None,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert_eq!(new_liquidation, target_liquidation);

    // Case 2: Cash-in more than PL

    let target_liquidation = Price::try_from(105_000.0).unwrap();
    assert!(target_liquidation > original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, -147_618);
    assert!(collateral_delta.abs() > original_pl as i64);

    let cash_in_amount = NonZeroU64::new(collateral_delta.abs() as u64).unwrap();

    let (_, _, _, new_liquidation, _) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        None,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    let liquidation_diff = (new_liquidation.into_f64() - target_liquidation.into_f64()).abs();
    assert!(
        liquidation_diff < 1.0,
        "Estimated liquidation distant from target by {liquidation_diff}",
    );
}

#[test]
fn test_collateral_delta_estimation_long_profit_leverage_down() {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.0).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let market_price = Price::try_from(110_000.0).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();

    assert_eq!(original_pl, 90_909.);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 90909.0);

    let target_liquidation = Price::try_from(85_000.0).unwrap();
    assert!(target_liquidation < original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, 76_471);

    let add_margin_amount = NonZeroU64::new(collateral_delta as u64).unwrap();

    let (_, _, new_liquidation) = evaluate_added_margin(
        side,
        quantity,
        original_price,
        original_margin,
        add_margin_amount,
    )
    .unwrap();

    let liquidation_diff = (new_liquidation.into_f64() - target_liquidation.into_f64()).abs();
    assert!(
        liquidation_diff < 1.0,
        "Estimated liquidation distant from target by {liquidation_diff}",
    );
}

#[test]
fn test_collateral_delta_estimation_short_profit_leverage_up() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.0).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let market_price = Price::try_from(90_000.0).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();
    assert_eq!(original_pl, 111_111.);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 111_111.0);

    // Case 1: Cash-in less than PL

    let target_liquidation = Price::try_from(105_000.0).unwrap();
    assert!(target_liquidation < original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, -52_380);
    assert!(collateral_delta.abs() < original_pl as i64);

    let cash_in_amount = NonZeroU64::new(collateral_delta.abs() as u64).unwrap();

    let (_, _, _, new_liquidation, _) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        None,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    assert_eq!(new_liquidation, target_liquidation);

    // Case 2: Cash-in more than PL

    let target_liquidation = Price::try_from(95_000.0).unwrap();
    assert!(target_liquidation < original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, -152_631);
    assert!(collateral_delta.abs() > original_pl as i64);

    let cash_in_amount = NonZeroU64::new(collateral_delta.abs() as u64).unwrap();

    let (_, _, _, new_liquidation, _) = evaluate_cash_in(
        side,
        quantity,
        original_margin,
        original_price,
        None,
        market_price,
        cash_in_amount,
    )
    .unwrap();

    let liquidation_diff = (new_liquidation.into_f64() - target_liquidation.into_f64()).abs();
    assert!(
        liquidation_diff < 1.0,
        "Estimated liquidation distant from target by {liquidation_diff}",
    );
}

#[test]
fn test_collateral_delta_estimation_short_profit_leverage_down() {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let original_price = Price::try_from(100_000.0).unwrap();
    let original_leverage = Leverage::try_from(10.0).unwrap();
    let original_margin = Margin::calculate(quantity, original_price, original_leverage);

    let market_price = Price::try_from(90_000.0).unwrap();

    let original_pl = estimate_pl(side, quantity, original_price, market_price).floor();

    assert_eq!(original_pl, 111_111.);

    let original_liquidation =
        estimate_liquidation_price(side, quantity, original_price, original_leverage);
    assert_eq!(original_liquidation.into_f64(), 111_111.0);

    let target_liquidation = Price::try_from(121_000.0).unwrap();
    assert!(target_liquidation > original_liquidation);

    let collateral_delta = evaluate_collateral_delta_for_liquidation(
        side,
        quantity,
        original_margin,
        original_price,
        original_liquidation,
        target_liquidation,
        market_price,
    )
    .unwrap();

    assert_eq!(collateral_delta, 73_554);

    let add_margin_amount = NonZeroU64::new(collateral_delta as u64).unwrap();

    let (_, _, new_liquidation) = evaluate_added_margin(
        side,
        quantity,
        original_price,
        original_margin,
        add_margin_amount,
    )
    .unwrap();

    let liquidation_diff = (new_liquidation.into_f64() - target_liquidation.into_f64()).abs();
    assert!(
        liquidation_diff < 1.0,
        "Estimated liquidation distant from target by {liquidation_diff}",
    );
}
