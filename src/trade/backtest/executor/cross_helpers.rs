use lnm_sdk::api_v3::models::{
    CrossLeverage, PercentageCapped, Price, Quantity, SATS_PER_BTC, TradeSide,
};

#[allow(dead_code)]
const CROSS_MAINTENANCE_MARGIN_RATE: f64 = 0.0015;

#[allow(dead_code)]
pub(super) fn cross_running_margin(
    quantity: Quantity,
    entry_price: Price,
    leverage: CrossLeverage,
) -> u64 {
    (quantity.as_f64() * SATS_PER_BTC / entry_price.as_f64() / leverage.as_u64() as f64).ceil()
        as u64
}

#[allow(dead_code)]
pub(super) fn cross_maintenance_margin(quantity: Quantity, entry_price: Price) -> u64 {
    (quantity.as_f64() * SATS_PER_BTC / entry_price.as_f64() * CROSS_MAINTENANCE_MARGIN_RATE).ceil()
        as u64
}

#[allow(dead_code)]
pub(super) fn cross_trading_fee(
    quantity: Quantity,
    execution_price: Price,
    fee_perc: PercentageCapped,
) -> u64 {
    let fee_calc = SATS_PER_BTC * fee_perc.as_f64() / 100.;
    (fee_calc * quantity.as_f64() / execution_price.as_f64()).floor() as u64
}

#[allow(dead_code)]
pub(super) fn estimate_cross_liquidation(
    quantity: i64,
    entry_price: Option<Price>,
    effective_collateral: u64,
) -> Option<Price> {
    let entry_price = entry_price?;
    let abs_quantity = abs_cross_quantity(quantity)?;
    let side = if quantity > 0 {
        TradeSide::Buy
    } else {
        TradeSide::Sell
    };

    Some(estimate_cross_liquidation_for_side(
        side,
        abs_quantity,
        entry_price,
        effective_collateral,
    ))
}

#[allow(dead_code)]
pub(super) fn estimate_cross_liquidation_for_side(
    side: TradeSide,
    quantity: Quantity,
    entry_price: Price,
    effective_collateral: u64,
) -> Price {
    let quantity = quantity.as_f64();
    let inverse_entry = 1.0 / entry_price.as_f64();
    let collateral_per_contract = effective_collateral as f64 / SATS_PER_BTC / quantity;
    let liquidation = match side {
        TradeSide::Buy => 1.0 / (inverse_entry + collateral_per_contract),
        TradeSide::Sell => 1.0 / (inverse_entry - collateral_per_contract).max(0.0),
    };

    Price::bounded(liquidation)
}

#[allow(dead_code)]
pub(super) fn aggregate_cross_entry_price(
    existing_quantity: i64,
    existing_entry_price: Price,
    added_quantity: i64,
    added_price: Price,
) -> Option<Price> {
    if existing_quantity == 0 || added_quantity == 0 {
        return None;
    }
    if existing_quantity.signum() != added_quantity.signum() {
        return None;
    }

    let existing_quantity = existing_quantity.unsigned_abs() as f64;
    let added_quantity = added_quantity.unsigned_abs() as f64;
    let total_quantity = existing_quantity + added_quantity;
    let weighted_inverse =
        existing_quantity / existing_entry_price.as_f64() + added_quantity / added_price.as_f64();

    Some(Price::bounded(total_quantity / weighted_inverse))
}

#[allow(dead_code)]
pub(super) fn abs_cross_quantity(quantity: i64) -> Option<Quantity> {
    if quantity == 0 {
        return None;
    }

    Quantity::try_from(quantity.unsigned_abs()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(value: f64) -> Price {
        Price::try_from(value).unwrap()
    }

    fn quantity(value: u64) -> Quantity {
        Quantity::try_from(value).unwrap()
    }

    #[test]
    fn simulated_cross_margin_helpers_use_inverse_contract_formulas() {
        let quantity = quantity(1_000);
        let entry_price = price(100_000.0);
        let leverage = CrossLeverage::try_from(10).unwrap();

        assert_eq!(
            cross_running_margin(quantity, entry_price, leverage),
            100_000
        );
        assert_eq!(cross_maintenance_margin(quantity, entry_price), 1_500);
        assert_eq!(
            cross_trading_fee(
                quantity,
                entry_price,
                PercentageCapped::try_from(0.1).unwrap(),
            ),
            1_000
        );
    }

    #[test]
    fn simulated_cross_liquidation_uses_account_collateral() {
        let quantity = quantity(1_000);
        let entry_price = price(100_000.0);
        let collateral = 500_000;

        let long_liquidation =
            estimate_cross_liquidation_for_side(TradeSide::Buy, quantity, entry_price, collateral);
        let short_liquidation =
            estimate_cross_liquidation_for_side(TradeSide::Sell, quantity, entry_price, collateral);

        assert!(long_liquidation.as_f64() < entry_price.as_f64());
        assert!(short_liquidation.as_f64() > entry_price.as_f64());
        assert_eq!(
            estimate_cross_liquidation(1_000, Some(entry_price), collateral),
            Some(long_liquidation)
        );
        assert_eq!(
            estimate_cross_liquidation(-1_000, Some(entry_price), collateral),
            Some(short_liquidation)
        );
        assert_eq!(
            estimate_cross_liquidation(0, Some(entry_price), collateral),
            None
        );
    }

    #[test]
    fn simulated_cross_aggregate_entry_price_uses_inverse_weighting() {
        let aggregated =
            aggregate_cross_entry_price(10, price(100_000.0), 10, price(200_000.0)).unwrap();

        assert!((aggregated.as_f64() - 133_333.5).abs() <= 0.5);
        assert_eq!(
            aggregate_cross_entry_price(10, price(100_000.0), -10, price(100_000.0),),
            None
        );
    }
}
