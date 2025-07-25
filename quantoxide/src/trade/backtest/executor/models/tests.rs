use super::*;

use chrono::Utc;

fn get_lnm_fee() -> BoundedPercentage {
    BoundedPercentage::try_from(0.1).unwrap()
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
