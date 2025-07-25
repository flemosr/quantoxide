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

#[test]
fn test_pl_estimate_and_price_from_pl() {
    // Test case 1: Buy side with profit

    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let start_price = Price::try_from(110_000).unwrap();
    let end_price = Price::try_from(120_000).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 75_757);
    assert_eq!(calculated_end_price, end_price);

    // Test case 2: Buy side with loss

    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1_000).unwrap();
    let start_price = Price::try_from(110_000).unwrap();
    let end_price = Price::try_from(105_000).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -43_291);
    assert_eq!(calculated_end_price, end_price);

    // Test case 3: Sell side with profit

    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let start_price = Price::try_from(110_000).unwrap();
    let end_price = Price::try_from(90_000).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, 202_020);
    assert_eq!(calculated_end_price, end_price);

    // Test case 4: Sell side with loss

    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1_000).unwrap();
    let start_price = Price::try_from(110_000).unwrap();
    let end_price = Price::try_from(115_000).unwrap();

    let pl = pl_estimate(side, quantity, start_price, end_price);
    let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

    assert_eq!(pl, -39_526);
    assert_eq!(calculated_end_price, end_price);
}
