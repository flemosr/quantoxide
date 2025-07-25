use std::num::NonZeroU64;

use super::{
    super::{
        BoundedPercentage, Leverage, Margin, Price, Quantity, SATS_PER_BTC, TradeSize,
        error::TradeValidationError,
    },
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

pub fn evaluate_open_trade_params(
    side: TradeSide,
    size: TradeSize,
    leverage: Leverage,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    fee_perc: BoundedPercentage,
) -> Result<(Quantity, Margin, Price, u64, u64), TradeValidationError> {
    let (quantity, margin) = size
        .to_quantity_and_margin(entry_price, leverage)
        .map_err(|e| TradeValidationError::Generic(e.to_string()))?;

    let liquidation = estimate_liquidation_price(side, quantity, entry_price, leverage);

    match side {
        TradeSide::Buy => {
            if stoploss < liquidation {
                return Err(TradeValidationError::StoplossBelowLiquidationLong {
                    stoploss,
                    liquidation,
                });
            }
            if stoploss >= entry_price {
                return Err(TradeValidationError::StoplossAboveEntryForLong {
                    stoploss,
                    entry_price,
                });
            }
            if takeprofit <= entry_price {
                return Err(TradeValidationError::TakeprofitBelowEntryForLong {
                    takeprofit,
                    entry_price,
                });
            }
        }
        TradeSide::Sell => {
            if stoploss > liquidation {
                return Err(TradeValidationError::StoplossAboveLiquidationShort {
                    stoploss,
                    liquidation,
                });
            }
            if stoploss <= entry_price {
                return Err(TradeValidationError::StoplossBelowEntryForShort {
                    stoploss,
                    entry_price,
                });
            }
            if takeprofit >= entry_price {
                return Err(TradeValidationError::TakeprofitAboveEntryForShort {
                    takeprofit,
                    entry_price,
                });
            }
        }
    };

    let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
    let opening_fee = (fee_calc * quantity.into_f64() / entry_price.into_f64()).floor() as u64;
    let closing_fee_reserved =
        (fee_calc * quantity.into_f64() / liquidation.into_f64()).floor() as u64;

    Ok((
        quantity,
        margin,
        liquidation,
        opening_fee,
        closing_fee_reserved,
    ))
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

pub fn evaluate_added_margin(
    side: TradeSide,
    quantity: Quantity,
    price: Price,
    current_margin: Margin,
    added_amount: NonZeroU64,
) -> Result<(Margin, Leverage, Price), TradeValidationError> {
    let new_margin = current_margin + added_amount.into();

    let new_leverage = Leverage::try_calculate(quantity, new_margin, price).map_err(|e| {
        TradeValidationError::Generic(format!(
            "added margin would result in invalid leverage: {e}"
        ))
    })?;

    let new_liquidation = estimate_liquidation_price(side, quantity, price, new_leverage);

    Ok((new_margin, new_leverage, new_liquidation))
}

#[cfg(test)]
mod tests;
