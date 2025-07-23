use super::{
    super::{Leverage, Price, Quantity, SATS_PER_BTC},
    TradeSide,
};

pub fn estimate_liquidation_price(
    side: TradeSide,
    quantity: Quantity,
    entry_price: Price,
    leverage: Leverage,
) -> Price {
    // The `Margin::try_calculate` shouldn't be used here since 'ceil' is
    // used there to achive a `Margin` that would result in the same `Quantity`
    // input via `Quantity::try_calculate`. Said rounding would reduce the
    // corresponding liquidation contraint
    // Here, `floor` is used in order to *understate* the margin, resulting in
    // a more conservative liquidation price. As of May 4 2025, this approach
    // matches liquidation values obtained via the LNM platform.

    let quantity = quantity.into_f64();
    let price = entry_price.into_f64();
    let leverage = leverage.into_f64();

    let a = 1.0 / price;

    let floored_margin = (quantity * SATS_PER_BTC / price / leverage).floor();
    let b = floored_margin / SATS_PER_BTC / quantity;

    // May result in `f64::INFINITY`
    let liquidation_calc = match side {
        TradeSide::Buy => 1.0 / (a + b),
        TradeSide::Sell => 1.0 / (a - b).max(0.),
    };

    Price::clamp_from(liquidation_calc)
}

pub fn pl_estimate(
    side: TradeSide,
    quantity: Quantity,
    start_price: Price,
    end_price: Price,
) -> i64 {
    let start_price = start_price.into_f64();
    let end_price = end_price.into_f64();

    let inverse_price_delta = match side {
        TradeSide::Buy => SATS_PER_BTC / start_price - SATS_PER_BTC / end_price,
        TradeSide::Sell => SATS_PER_BTC / end_price - SATS_PER_BTC / start_price,
    };

    (quantity.into_f64() * inverse_price_delta).floor() as i64
}

pub fn price_from_pl(side: TradeSide, quantity: Quantity, start_price: Price, pl: i64) -> Price {
    let start_price = start_price.into_f64();
    let quantity = quantity.into_f64();

    let inverse_price_delta = (pl as f64) / quantity;

    let inverse_end_price = match side {
        TradeSide::Buy => (SATS_PER_BTC / start_price) - inverse_price_delta,
        TradeSide::Sell => (SATS_PER_BTC / start_price) + inverse_price_delta,
    };

    Price::clamp_from(SATS_PER_BTC / inverse_end_price)
}

#[cfg(test)]
mod tests {
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
}
