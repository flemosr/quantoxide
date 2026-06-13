use lnm_sdk::api_v3::models::{PercentageCapped, Price, SATS_PER_BTC, TradeSide};

use crate::trade::CrossQuantity;

pub(super) fn cross_trading_fee(
    quantity: CrossQuantity,
    execution_price: Price,
    fee_perc: PercentageCapped,
) -> i64 {
    let fee_calc = SATS_PER_BTC * fee_perc.as_f64() / 100.;
    (fee_calc * quantity.as_f64() / execution_price.as_f64()).floor() as i64
}

pub(super) fn aggregate_cross_entry_price(
    existing_quantity: CrossQuantity,
    existing_entry_price: Price,
    added_quantity: CrossQuantity,
    added_price: Price,
) -> Price {
    let existing_quantity = existing_quantity.as_f64();
    let added_quantity = added_quantity.as_f64();
    let total_quantity = existing_quantity + added_quantity;
    let weighted_inverse =
        existing_quantity / existing_entry_price.as_f64() + added_quantity / added_price.as_f64();

    Price::bounded(total_quantity / weighted_inverse)
}

pub(super) fn estimate_cross_pl(
    side: TradeSide,
    quantity: CrossQuantity,
    start_price: Price,
    end_price: Price,
) -> f64 {
    let start_price = start_price.as_f64();
    let end_price = end_price.as_f64();

    let inverse_price_delta = match side {
        TradeSide::Buy => SATS_PER_BTC / start_price - SATS_PER_BTC / end_price,
        TradeSide::Sell => SATS_PER_BTC / end_price - SATS_PER_BTC / start_price,
    };

    quantity.as_f64() * inverse_price_delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_cross_aggregate_entry_price_uses_inverse_weighting() {
        let aggregated = aggregate_cross_entry_price(
            10.try_into().unwrap(),
            Price::bounded(100_000.0),
            10.try_into().unwrap(),
            Price::bounded(200_000.0),
        );

        assert!((aggregated.as_f64() - 133_333.5).abs() <= 0.5);
    }
}
