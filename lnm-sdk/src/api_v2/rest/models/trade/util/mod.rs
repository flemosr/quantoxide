use std::num::NonZeroU64;

use crate::shared::models::{
    SATS_PER_BTC,
    error::TradeValidationError,
    leverage::Leverage,
    margin::Margin,
    price::{BoundedPercentage, Price},
    quantity::Quantity,
    trade::TradeSide,
};

use super::super::trade::TradeSize;

/// Estimates the liquidation price for a trade position.
///
/// Calculates the price at which a position would be liquidated based on the trade parameters.
/// Uses a conservative calculation with floored margin to understate the margin, resulting in a
/// more conservative liquidation price that matches values from the LNM platform.
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

/// Evaluates and validates parameters for opening a new trade.
///
/// Validates all trade parameters including stop-loss and take-profit levels against the
/// liquidation price and entry price. Calculates the quantity, margin, liquidation price, and
/// trading fees.
pub fn evaluate_open_trade_params(
    side: TradeSide,
    size: TradeSize,
    leverage: Leverage,
    entry_price: Price,
    stoploss: Option<Price>,
    takeprofit: Option<Price>,
    fee_perc: BoundedPercentage,
) -> Result<(Quantity, Margin, Price, u64, u64), TradeValidationError> {
    let (quantity, margin) = size
        .to_quantity_and_margin(entry_price, leverage)
        .map_err(TradeValidationError::TradeParamsInvalidQuantity)?;

    let liquidation = estimate_liquidation_price(side, quantity, entry_price, leverage);

    match side {
        TradeSide::Buy => {
            if let Some(stoploss) = stoploss {
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
            }
            if let Some(takeprofit) = takeprofit {
                if takeprofit <= entry_price {
                    return Err(TradeValidationError::TakeprofitBelowEntryForLong {
                        takeprofit,
                        entry_price,
                    });
                }
            }
        }
        TradeSide::Sell => {
            if let Some(stoploss) = stoploss {
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
            }
            if let Some(takeprofit) = takeprofit {
                if takeprofit >= entry_price {
                    return Err(TradeValidationError::TakeprofitAboveEntryForShort {
                        takeprofit,
                        entry_price,
                    });
                }
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

/// Estimates the profit/loss for a position between two prices.
///
/// Calculates the profit or loss in satoshis for a position moving from a start price to an end
/// price.
pub fn estimate_pl(
    side: TradeSide,
    quantity: Quantity,
    start_price: Price,
    end_price: Price,
) -> f64 {
    let start_price = start_price.into_f64();
    let end_price = end_price.into_f64();

    let inverse_price_delta = match side {
        TradeSide::Buy => SATS_PER_BTC / start_price - SATS_PER_BTC / end_price,
        TradeSide::Sell => SATS_PER_BTC / end_price - SATS_PER_BTC / start_price,
    };

    quantity.into_f64() * inverse_price_delta
}

/// Estimates the price corresponding to a specific profit/loss amount.
///
/// Given a starting price and a target P/L in satoshis, calculates what the end price would need to
/// be to achieve that profit or loss.
pub fn estimate_price_from_pl(
    side: TradeSide,
    quantity: Quantity,
    start_price: Price,
    pl: f64,
) -> Price {
    let start_price = start_price.into_f64();
    let quantity = quantity.into_f64();

    let inverse_price_delta = pl / quantity;

    let inverse_end_price = match side {
        TradeSide::Buy => (SATS_PER_BTC / start_price) - inverse_price_delta,
        TradeSide::Sell => (SATS_PER_BTC / start_price) + inverse_price_delta,
    };

    Price::clamp_from(SATS_PER_BTC / inverse_end_price)
}

/// Validates a new stop-loss price for an existing trade.
///
/// Ensures the new stop-loss price is valid relative to the liquidation price, current market
/// price, and any existing take-profit level.
pub fn evaluate_new_stoploss(
    side: TradeSide,
    liquidation: Price,
    takeprofit: Option<Price>,
    market_price: Price,
    new_stoploss: Price,
) -> Result<(), TradeValidationError> {
    match side {
        TradeSide::Buy => {
            if new_stoploss < liquidation {
                return Err(TradeValidationError::StoplossBelowLiquidationLong {
                    stoploss: new_stoploss,
                    liquidation,
                });
            }
            if new_stoploss >= market_price {
                return Err(TradeValidationError::NewStoplossNotBelowMarketForLong {
                    new_stoploss,
                    market_price,
                });
            }
            if let Some(takeprofit) = takeprofit {
                if new_stoploss >= takeprofit {
                    return Err(TradeValidationError::NewStoplossNotBelowTakeprofitForLong {
                        new_stoploss,
                        takeprofit,
                    });
                }
            }
        }
        TradeSide::Sell => {
            if new_stoploss > liquidation {
                return Err(TradeValidationError::StoplossAboveLiquidationShort {
                    stoploss: new_stoploss,
                    liquidation,
                });
            }
            if new_stoploss <= market_price {
                return Err(TradeValidationError::NewStoplossNotAboveMarketForShort {
                    new_stoploss,
                    market_price,
                });
            }
            if let Some(takeprofit) = takeprofit {
                if new_stoploss <= takeprofit {
                    return Err(
                        TradeValidationError::NewStoplossNotAboveTakeprofitForShort {
                            new_stoploss,
                            takeprofit,
                        },
                    );
                }
            }
        }
    }

    Ok(())
}

/// Evaluates the impact of adding margin to an existing trade.
///
/// Calculates the new margin, leverage, and liquidation price that would result from adding
/// additional collateral to a position.
pub fn evaluate_added_margin(
    side: TradeSide,
    quantity: Quantity,
    price: Price,
    current_margin: Margin,
    amount: NonZeroU64,
) -> Result<(Margin, Leverage, Price), TradeValidationError> {
    let new_margin = current_margin + amount.into();

    let new_leverage = Leverage::try_calculate(quantity, new_margin, price)
        .map_err(TradeValidationError::AddedMarginInvalidLeverage)?;

    let new_liquidation = estimate_liquidation_price(side, quantity, price, new_leverage);

    Ok((new_margin, new_leverage, new_liquidation))
}

/// Evaluates the impact of cashing in profit and/or margin from a trade.
///
/// Calculates how extracting a specified amount affects the trade's entry price, margin,
/// leverage, and liquidation price. First extracts available profit, then margin if needed.
/// Updates or clears the stop-loss if it becomes invalid after the cash-in.
pub fn evaluate_cash_in(
    side: TradeSide,
    quantity: Quantity,
    margin: Margin,
    price: Price,
    stoploss: Option<Price>,
    market_price: Price,
    amount: NonZeroU64,
) -> Result<(Price, Margin, Leverage, Price, Option<Price>), TradeValidationError> {
    let amount = amount.get() as u64;
    let current_pl = estimate_pl(side, quantity, price, market_price);

    let (new_price, remaining_amount) = if current_pl > 0. {
        if amount < current_pl as u64 {
            // PL should be partially cashed-in. Calculate price that would
            // correspond to the PL that will be extracted.
            let new_price = estimate_price_from_pl(side, quantity, price, amount as f64);
            (new_price, 0)
        } else {
            // Whole PL should be cashed-in. Adjust trade price to market price
            (market_price, amount - current_pl as u64)
        }
    } else {
        // No PL to be cashed-in. Trade price shouldn't change
        (price, amount)
    };

    let new_margin = if remaining_amount == 0 {
        // Only PL will be cashed-in. Margin shouldn't change
        margin
    } else {
        Margin::try_from(margin.into_u64().saturating_sub(remaining_amount))
            .map_err(TradeValidationError::CashInInvalidMargin)?
    };

    let new_leverage = Leverage::try_calculate(quantity, new_margin, new_price)
        .map_err(TradeValidationError::CashInInvalidLeverage)?;
    let new_liquidation = estimate_liquidation_price(side, quantity, new_price, new_leverage);

    let new_stoploss = stoploss.and_then(|sl| {
        let valid = match side {
            TradeSide::Buy => new_liquidation <= sl,
            TradeSide::Sell => new_liquidation >= sl,
        };

        if valid { Some(sl) } else { None }
    });

    Ok((
        new_price,
        new_margin,
        new_leverage,
        new_liquidation,
        new_stoploss,
    ))
}

/// Calculates the collateral change needed to reach a target liquidation price.
///
/// Determines how much collateral needs to be added (positive) or removed (negative) to move the
/// liquidation price to a target level, accounting for current profit/loss.
pub fn evaluate_collateral_delta_for_liquidation(
    side: TradeSide,
    quantity: Quantity,
    margin: Margin,
    price: Price,
    liquidation: Price,
    target_liquidation: Price,
    market_price: Price,
) -> Result<i64, TradeValidationError> {
    if target_liquidation == liquidation {
        return Ok(0);
    }

    // Margin needed for `target_liquidation`, at the current `market_price`
    let target_collateral =
        Margin::est_from_liquidation_price(side, quantity, market_price, target_liquidation)?;

    let pl = estimate_pl(side, quantity, price, market_price);

    // target collateral - current collateral
    let colateral_diff = target_collateral.into_i64() - margin.into_i64() - pl.round() as i64;

    Ok(colateral_diff)
}

/// Calculates the closing fee for a trade at a given price.
///
/// Computes the trading fee in satoshis that would be charged for closing a position at the
/// specified price.
pub fn evaluate_closing_fee(
    fee_perc: BoundedPercentage,
    quantity: Quantity,
    close_price: Price,
) -> u64 {
    let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
    let closing_fee = (fee_calc * quantity.into_f64() / close_price.into_f64()).floor() as u64;
    closing_fee
}

#[cfg(test)]
mod tests;
