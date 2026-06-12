use std::{num::NonZeroU64, sync::Arc};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api_v3::{
    error::LeverageValidationError,
    models::{
        ClientId, CrossLeverage, Leverage, Margin, PercentageCapped, Price, Quantity, SATS_PER_BTC,
        TradeSide, TradeSize, trade_util,
    },
};

use crate::{
    db::models::{FundingSettlementRow, OhlcCandleRow},
    trade::backtest::executor::cross_helpers::estimate_cross_liquidation_for_side,
};

use super::{
    super::super::core::{
        CrossExposure, CrossPositionCore, CrossQuantity, TradeClosed, TradeCore, TradeRunning,
    },
    cross_helpers::{
        aggregate_cross_entry_price, cross_maintenance_margin, cross_running_margin,
        cross_trading_fee, estimate_cross_pl,
    },
    error::{SimulatedTradeExecutorError, SimulatedTradeExecutorResult},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SimulatedCrossPosition {
    margin: u64,
    leverage: CrossLeverage,
    exposure: CrossExposure,
    trading_fees: u64,
    session_funding_fees: i64,
}

impl SimulatedCrossPosition {
    pub fn new(
        market_price: f64,
        margin: u64,
        leverage: CrossLeverage,
        exposure: CrossExposure,
        trading_fees: u64,
        session_funding_fees: i64,
    ) -> SimulatedTradeExecutorResult<Self> {
        let state = Self {
            margin,
            leverage,
            exposure,
            trading_fees,
            session_funding_fees,
        };

        if matches!(state.exposure, CrossExposure::Running { .. })
            && state.est_free_margin(Price::bounded(market_price)) == 0
        {
            return Err(SimulatedTradeExecutorError::CrossMarginTooLow);
        }

        Ok(state)
    }

    pub fn initial(start_candle: &OhlcCandleRow) -> Self {
        Self::new(
            start_candle.open,
            0,
            CrossLeverage::MIN,
            CrossExposure::Neutral,
            0,
            0,
        )
        .expect("initial simulated cross position must be valid")
    }

    pub fn from_running_params(
        market_price: f64,
        margin: u64,
        leverage: CrossLeverage,
        side: TradeSide,
        quantity: CrossQuantity,
        entry_price: Price,
        trading_fees: u64,
        session_funding_fees: i64,
    ) -> SimulatedTradeExecutorResult<Self> {
        let exposure =
            Self::evaluate_running_exposure(margin, leverage, side, quantity, entry_price)?;
        Self::new(
            market_price,
            margin,
            leverage,
            exposure,
            trading_fees,
            session_funding_fees,
        )
    }

    fn evaluate_running_exposure(
        margin: u64,
        leverage: CrossLeverage,
        side: TradeSide,
        quantity: CrossQuantity,
        entry_price: Price,
    ) -> SimulatedTradeExecutorResult<CrossExposure> {
        let liquidation = estimate_cross_liquidation_for_side(side, quantity, entry_price, margin);
        let running_margin =
            Margin::try_from(cross_running_margin(quantity, entry_price, leverage))
                .map_err(|_| SimulatedTradeExecutorError::CrossMarginTooLow)?;
        let maintenance_margin = Margin::try_from(cross_maintenance_margin(quantity, entry_price))
            .map_err(|_| SimulatedTradeExecutorError::CrossMarginTooLow)?;

        Ok(CrossExposure::Running {
            quantity,
            side,
            entry_price,
            liquidation,
            running_margin,
            maintenance_margin,
        })
    }

    pub fn with_margin(
        &self,
        market_price: f64,
        new_margin: u64,
    ) -> SimulatedTradeExecutorResult<Self> {
        let new_exposure = match self.exposure {
            CrossExposure::Neutral => CrossExposure::Neutral,
            CrossExposure::Running {
                quantity,
                side,
                entry_price,
                running_margin,
                maintenance_margin,
                ..
            } => {
                let new_liquidation =
                    estimate_cross_liquidation_for_side(side, quantity, entry_price, new_margin);

                CrossExposure::Running {
                    quantity,
                    side,
                    entry_price,
                    liquidation: new_liquidation,
                    running_margin,
                    maintenance_margin,
                }
            }
        };

        Self::new(
            market_price,
            new_margin,
            self.leverage,
            new_exposure,
            self.trading_fees,
            self.session_funding_fees,
        )
    }

    pub fn with_leverage(
        &self,
        market_price: f64,
        new_leverage: CrossLeverage,
    ) -> SimulatedTradeExecutorResult<Self> {
        let new_exposure = match self.exposure {
            CrossExposure::Neutral => CrossExposure::Neutral,
            CrossExposure::Running {
                quantity,
                side,
                entry_price,
                liquidation,
                maintenance_margin,
                ..
            } => {
                let new_running_margin =
                    Margin::try_from(cross_running_margin(quantity, entry_price, new_leverage))
                        .map_err(|_| SimulatedTradeExecutorError::CrossMarginTooLow)?;

                CrossExposure::Running {
                    quantity,
                    side,
                    entry_price,
                    liquidation,
                    running_margin: new_running_margin,
                    maintenance_margin,
                }
            }
        };

        Self::new(
            market_price,
            self.margin,
            new_leverage,
            new_exposure,
            self.trading_fees,
            self.session_funding_fees,
        )
    }

    pub fn with_market_order(
        &self,
        market_price: Price,
        order_side: TradeSide,
        order_quantity: CrossQuantity,
        fee_perc: PercentageCapped,
    ) -> SimulatedTradeExecutorResult<Self> {
        let order_fee = cross_trading_fee(order_quantity, market_price, fee_perc);
        let margin_after_order_fee = self
            .margin
            .checked_sub(order_fee)
            .ok_or(SimulatedTradeExecutorError::CrossFreeMarginTooLow)?;
        let cumulative_trading_fees = self
            .trading_fees
            .checked_add(order_fee)
            .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?;

        let CrossExposure::Running {
            quantity: current_quantity,
            side: current_side,
            entry_price: current_entry_price,
            ..
        } = self.exposure
        else {
            // Opening from flat uses the execution price as the new entry price.
            return Self::from_running_params(
                market_price.as_f64(),
                margin_after_order_fee,
                self.leverage,
                order_side,
                order_quantity,
                market_price,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        };

        // Same-side orders increase exposure and update the weighted entry price.
        if current_side == order_side {
            let resulting_quantity =
                CrossQuantity::try_from(current_quantity.as_u64() + order_quantity.as_u64())
                    .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;
            let resulting_entry_price = aggregate_cross_entry_price(
                current_quantity,
                current_entry_price,
                order_quantity,
                market_price,
            );

            return Self::from_running_params(
                market_price.as_f64(),
                margin_after_order_fee,
                self.leverage,
                order_side,
                resulting_quantity,
                resulting_entry_price,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        }

        // Order doesn't have the same side as current position

        let reduced_quantity =
            CrossQuantity::try_from(current_quantity.as_u64().min(order_quantity.as_u64()))
                .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;
        let realized_reduction_pl = estimate_cross_pl(
            current_side,
            reduced_quantity,
            current_entry_price,
            market_price,
        )
        .floor() as i64;

        let apply_realized_pl_to_margin =
            |margin: u64, realized_pl: i64| -> SimulatedTradeExecutorResult<u64> {
                if realized_pl >= 0 {
                    margin
                        .checked_add(realized_pl as u64)
                        .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)
                } else {
                    margin
                        .checked_sub(realized_pl.unsigned_abs())
                        .ok_or(SimulatedTradeExecutorError::CrossMarginTooLow)
                }
            };

        // Exact close resets position-only fields and books realized P/L into margin.
        if current_quantity == order_quantity {
            let margin =
                apply_realized_pl_to_margin(margin_after_order_fee, realized_reduction_pl)?;
            return Self::new(
                market_price.as_f64(),
                margin,
                self.leverage,
                CrossExposure::Neutral,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        }

        let resulting_side = if current_quantity < order_quantity {
            order_side
        } else {
            current_side
        };
        let resulting_quantity =
            CrossQuantity::try_from(current_quantity.as_u64().abs_diff(order_quantity.as_u64()))
                .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;

        // Reversals book the old position P/L and start the residual side at execution price.
        if resulting_side != current_side {
            let margin =
                apply_realized_pl_to_margin(margin_after_order_fee, realized_reduction_pl)?;

            return Self::from_running_params(
                market_price.as_f64(),
                margin,
                self.leverage,
                resulting_side,
                resulting_quantity,
                market_price,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        }

        // Break-even partial reduction: fee is already paid, entry is unchanged.
        if realized_reduction_pl == 0 {
            return Self::from_running_params(
                market_price.as_f64(),
                margin_after_order_fee,
                self.leverage,
                resulting_side,
                resulting_quantity,
                current_entry_price,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        }

        // Profitable partial reductions realize P/L into margin and keep entry unchanged.
        if realized_reduction_pl > 0 {
            let margin =
                apply_realized_pl_to_margin(margin_after_order_fee, realized_reduction_pl)?;
            return Self::from_running_params(
                market_price.as_f64(),
                margin,
                self.leverage,
                resulting_side,
                resulting_quantity,
                current_entry_price,
                cumulative_trading_fees,
                self.session_funding_fees,
            );
        }

        // Losing partial reductions carry the loss in the remaining position entry price.
        let full_position_pl = estimate_cross_pl(
            current_side,
            current_quantity,
            current_entry_price,
            market_price,
        );
        let inverse_market_price = SATS_PER_BTC / market_price.as_f64();
        let carried_inverse_entry_price = match current_side {
            TradeSide::Buy => inverse_market_price + full_position_pl / resulting_quantity.as_f64(),
            TradeSide::Sell => {
                inverse_market_price - full_position_pl / resulting_quantity.as_f64()
            }
        };
        let carried_entry_price = Price::bounded(SATS_PER_BTC / carried_inverse_entry_price);

        Self::from_running_params(
            market_price.as_f64(),
            margin_after_order_fee,
            self.leverage,
            resulting_side,
            resulting_quantity,
            carried_entry_price,
            cumulative_trading_fees,
            self.session_funding_fees,
        )
    }

    #[allow(dead_code)]
    pub fn session_funding_fees(&self) -> i64 {
        self.session_funding_fees
    }
}

impl crate::sealed::Sealed for SimulatedCrossPosition {}

impl CrossPositionCore for SimulatedCrossPosition {
    fn margin(&self) -> u64 {
        self.margin
    }

    fn leverage(&self) -> CrossLeverage {
        self.leverage
    }

    fn exposure(&self) -> CrossExposure {
        self.exposure
    }

    fn trading_fees(&self) -> u64 {
        self.trading_fees
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SimulatedTradeRunning {
    id: Uuid,
    side: TradeSide,
    opening_fee: u64,
    closing_fee_reserved: u64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    stoploss: Option<Price>,
    takeprofit: Option<Price>,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    client_id: Option<ClientId>,
}

impl SimulatedTradeRunning {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        entry_time: DateTime<Utc>,
        entry_price: Price,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        fee_perc: PercentageCapped,
        client_id: Option<ClientId>,
    ) -> SimulatedTradeExecutorResult<Arc<Self>> {
        let (quantity, margin, liquidation, opening_fee, closing_fee_reserved) =
            trade_util::evaluate_open_trade_params(
                side,
                size,
                leverage,
                entry_price,
                stoploss,
                takeprofit,
                fee_perc,
            )
            .map_err(SimulatedTradeExecutorError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: Uuid::new_v4(),
            side,
            opening_fee,
            closing_fee_reserved,
            quantity,
            margin,
            leverage,
            price: entry_price,
            liquidation,
            stoploss,
            takeprofit,
            entry_time,
            entry_price,
            client_id,
        }))
    }

    pub fn with_new_stoploss(
        &self,
        market_price: Price,
        new_stoploss: Price,
    ) -> SimulatedTradeExecutorResult<Arc<Self>> {
        trade_util::evaluate_new_stoploss(
            self.side,
            self.liquidation,
            self.takeprofit,
            market_price,
            new_stoploss,
        )
        .map_err(SimulatedTradeExecutorError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            stoploss: Some(new_stoploss),
            takeprofit: self.takeprofit,
            margin: self.margin,
            quantity: self.quantity,
            leverage: self.leverage,
            liquidation: self.liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
            client_id: self.client_id.clone(),
        }))
    }

    pub fn with_added_margin(&self, amount: NonZeroU64) -> SimulatedTradeExecutorResult<Arc<Self>> {
        let (new_margin, new_leverage, new_liquidation) = trade_util::evaluate_added_margin(
            self.side,
            self.quantity,
            self.price,
            self.margin,
            amount,
        )
        .map_err(SimulatedTradeExecutorError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            stoploss: self.stoploss,
            takeprofit: self.takeprofit,
            margin: new_margin,
            quantity: self.quantity,
            leverage: new_leverage,
            liquidation: new_liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
            client_id: self.client_id.clone(),
        }))
    }

    pub fn with_cash_in(
        &self,
        market_price: Price,
        amount: NonZeroU64,
    ) -> SimulatedTradeExecutorResult<Arc<Self>> {
        let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) =
            trade_util::evaluate_cash_in(
                self.side,
                self.quantity,
                self.margin,
                self.price,
                self.stoploss,
                market_price,
                amount,
            )
            .map_err(SimulatedTradeExecutorError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: new_price,
            stoploss: new_stoploss,
            takeprofit: self.takeprofit,
            margin: new_margin,
            quantity: self.quantity,
            leverage: new_leverage,
            liquidation: new_liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
            client_id: self.client_id.clone(),
        }))
    }

    /// Applies a funding settlement to this trade, updating margin, leverage, and liquidation.
    ///
    /// Returns `Some(updated_trade)` when the trade can be updated, or `None` when margin or
    /// leverage became invalid (trade is effectively bankrupt). Positive fees (cost) are deducted
    /// from margin. Negative fees (revenue) should be added to the balance.
    ///
    /// This method does NOT check whether the new liquidation price crosses the market price.
    /// That check is left to the next `candle_update`, which will liquidate the trade through the
    /// normal price-trigger mechanism.
    pub fn apply_funding_settlement(
        &self,
        settlement: &FundingSettlementRow,
    ) -> SimulatedTradeExecutorResult<(Option<Arc<Self>>, i64)> {
        let raw_fee = (self.quantity.as_f64() / settlement.fixing_price)
            * settlement.funding_rate
            * SATS_PER_BTC;

        // Positive = cost (paid), negative = revenue (received)
        // Longs pay when funding rates are positive, shorts pay when negative
        let funding_fee = match self.side {
            TradeSide::Buy => raw_fee,
            TradeSide::Sell => -raw_fee,
        }
        .round() as i64;

        if funding_fee <= 0 {
            return Ok((Some(Arc::new(self.clone())), funding_fee));
        }

        let Ok(new_margin) = Margin::try_from(self.margin.as_i64() - funding_fee) else {
            return Ok((None, funding_fee));
        };

        // A sub-MIN result is a benign rounding artifact from the open-time quantity flooring.
        // LN Markets accepts these overcollateralized positions, so `new_leverage` is clamped to
        // `Leverage::MIN` and the trade is kept running. An above-MAX result means the funding fee
        // has eroded the margin past the `Leverage::MAX` threshold, so the trade is force-closed.
        let new_leverage = match Leverage::try_calculate(self.quantity, new_margin, self.price) {
            Ok(leverage) => leverage,
            Err(LeverageValidationError::TooLow { .. }) => Leverage::MIN,
            Err(LeverageValidationError::TooHigh { .. }) => return Ok((None, funding_fee)),
        };

        let new_liquidation = trade_util::estimate_liquidation_price(
            self.side,
            self.quantity,
            self.price,
            new_leverage,
        );

        Ok((
            Some(Arc::new(Self {
                margin: new_margin,
                leverage: new_leverage,
                liquidation: new_liquidation,
                ..self.clone()
            })),
            funding_fee,
        ))
    }

    pub fn to_closed(
        &self,
        fee_perc: PercentageCapped,
        close_time: DateTime<Utc>,
        close_price: Price,
    ) -> Arc<SimulatedTradeClosed> {
        let closing_fee = trade_util::evaluate_closing_fee(fee_perc, self.quantity, close_price);

        Arc::new(SimulatedTradeClosed {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            liquidation: self.liquidation,
            stoploss: self.stoploss,
            takeprofit: self.takeprofit,
            margin: self.margin,
            quantity: self.quantity,
            leverage: self.leverage,
            close_time,
            close_price,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
            closing_fee,
            client_id: self.client_id.clone(),
        })
    }
}

impl TradeCore for SimulatedTradeRunning {
    fn id(&self) -> Uuid {
        self.id
    }

    fn side(&self) -> TradeSide {
        self.side
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn closing_fee(&self) -> u64 {
        0
    }

    fn maintenance_margin(&self) -> i64 {
        self.opening_fee as i64 + self.closing_fee_reserved as i64
    }

    fn quantity(&self) -> Quantity {
        self.quantity
    }

    fn margin(&self) -> Margin {
        self.margin
    }

    fn leverage(&self) -> Leverage {
        self.leverage
    }

    fn price(&self) -> Price {
        self.price
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    fn exit_price(&self) -> Option<Price> {
        None
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn filled_at(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_at(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn closed(&self) -> bool {
        false
    }

    fn client_id(&self) -> Option<&ClientId> {
        self.client_id.as_ref()
    }
}

impl crate::sealed::Sealed for SimulatedTradeRunning {}

impl TradeRunning for SimulatedTradeRunning {
    fn est_pl(&self, market_price: Price) -> f64 {
        trade_util::estimate_pl(self.side(), self.quantity(), self.price(), market_price)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SimulatedTradeClosed {
    id: Uuid,
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    price: Price,
    liquidation: Price,
    stoploss: Option<Price>,
    takeprofit: Option<Price>,
    margin: Margin,
    quantity: Quantity,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
    opening_fee: u64,
    closing_fee_reserved: u64,
    closing_fee: u64,
    client_id: Option<ClientId>,
}

impl TradeCore for SimulatedTradeClosed {
    fn id(&self) -> Uuid {
        self.id
    }

    fn side(&self) -> TradeSide {
        self.side
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    fn maintenance_margin(&self) -> i64 {
        self.opening_fee as i64 + self.closing_fee_reserved as i64
    }

    fn quantity(&self) -> Quantity {
        self.quantity
    }

    fn margin(&self) -> Margin {
        self.margin
    }

    fn leverage(&self) -> Leverage {
        self.leverage
    }

    fn price(&self) -> Price {
        self.price
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    fn exit_price(&self) -> Option<Price> {
        Some(self.close_price)
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn filled_at(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_at(&self) -> Option<DateTime<Utc>> {
        Some(self.close_time)
    }

    fn closed(&self) -> bool {
        true
    }

    fn client_id(&self) -> Option<&ClientId> {
        self.client_id.as_ref()
    }
}

impl crate::sealed::Sealed for SimulatedTradeClosed {}

impl TradeClosed for SimulatedTradeClosed {
    fn pl(&self) -> i64 {
        trade_util::estimate_pl(self.side(), self.quantity(), self.price(), self.close_price)
            .floor() as i64
    }
}
