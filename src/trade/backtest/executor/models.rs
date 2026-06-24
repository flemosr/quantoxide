use std::{cmp::Ordering, num::NonZeroU64, sync::Arc};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api_v3::{
    error::LeverageValidationError,
    models::{
        ClientId, CrossExposure, CrossLeverage, CrossQuantity, Leverage, Margin, OrderQuantity,
        PercentageCapped, Price, SATS_PER_BTC, TradeSide, TradeSize, trade_util,
    },
};

use crate::db::models::{FundingSettlementRow, OhlcCandleRow};

use super::{
    super::super::core::{CrossPositionCore, TradeClosed, TradeCore, TradeRunning},
    error::{SimulatedTradeExecutorError, SimulatedTradeExecutorResult},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SimulatedCrossPosition {
    margin: u64,
    leverage: CrossLeverage,
    exposure: CrossExposure,
    trading_fees: u64,
    session_funding_fees: i64,
    realized_pl: i64,
}

impl SimulatedCrossPosition {
    pub fn new(
        margin: u64,
        leverage: CrossLeverage,
        exposure_running: Option<(TradeSide, CrossQuantity, Price)>,
        trading_fees: u64,
        session_funding_fees: i64,
        realized_pl: i64,
    ) -> SimulatedTradeExecutorResult<Self> {
        let exposure = CrossExposure::new(margin, leverage, exposure_running)
            .map_err(SimulatedTradeExecutorError::CrossExposureValidation)?;

        Ok(Self {
            margin,
            leverage,
            exposure,
            trading_fees,
            session_funding_fees,
            realized_pl,
        })
    }

    pub fn initial() -> Self {
        Self::new(0, CrossLeverage::MIN, None, 0, 0, 0)
            .expect("must be valid `CrossPosition` params")
    }

    fn apply_amount_to_margin(margin: u64, amount: i64) -> SimulatedTradeExecutorResult<u64> {
        if amount < 0 {
            Ok(margin.saturating_sub(amount.unsigned_abs()))
        } else {
            margin
                .checked_add(amount.unsigned_abs())
                .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)
        }
    }

    pub fn is_coherent(&self, market_price: Price) -> bool {
        match self.exposure {
            CrossExposure::Neutral => true,
            CrossExposure::Running(cross_exposure_running) => {
                let is_liquid = match cross_exposure_running.side() {
                    TradeSide::Buy => market_price > cross_exposure_running.liquidation(),
                    TradeSide::Sell => market_price < cross_exposure_running.liquidation(),
                };

                is_liquid && self.est_free_margin(market_price) > 0
            }
        }
    }

    pub fn with_margin(&self, new_margin: u64) -> SimulatedTradeExecutorResult<Self> {
        Self::new(
            new_margin,
            self.leverage,
            self.exposure.as_running_params(),
            self.trading_fees,
            self.session_funding_fees,
            self.realized_pl,
        )
    }

    pub fn with_leverage(&self, new_leverage: CrossLeverage) -> SimulatedTradeExecutorResult<Self> {
        Self::new(
            self.margin,
            new_leverage,
            self.exposure.as_running_params(),
            self.trading_fees,
            self.session_funding_fees,
            self.realized_pl,
        )
    }

    pub fn apply_funding_settlement(
        &self,
        market_price: f64,
        settlement: &FundingSettlementRow,
        fee_perc: PercentageCapped,
    ) -> SimulatedTradeExecutorResult<(Self, i64, bool)> {
        let CrossExposure::Running(exposure) = self.exposure else {
            return Ok((*self, 0, false));
        };
        let quantity = exposure.quantity();
        let side = exposure.side();
        let market_price = Price::bounded(market_price);
        let raw_fee =
            (quantity.as_f64() / settlement.fixing_price) * settlement.funding_rate * SATS_PER_BTC;
        let funding_fee = match side {
            TradeSide::Buy => raw_fee,
            TradeSide::Sell => -raw_fee,
        }
        .round() as i64;
        let new_session_funding_fees = self
            .session_funding_fees
            .checked_add(funding_fee)
            .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?;

        let update_margin = |margin: u64| -> SimulatedTradeExecutorResult<u64> {
            Self::apply_amount_to_margin(
                margin,
                funding_fee
                    .checked_neg()
                    .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?,
            )
        };

        let force_close = || -> SimulatedTradeExecutorResult<_> {
            let closed_position = self.close(market_price, fee_perc)?;
            let remaining_margin = update_margin(closed_position.margin)?;
            let final_position = Self::new(
                remaining_margin,
                closed_position.leverage,
                None,
                closed_position.trading_fees,
                new_session_funding_fees,
                closed_position.realized_pl,
            )?;

            Ok((final_position, funding_fee, true))
        };

        if funding_fee > 0 && funding_fee as u64 >= self.est_free_margin(market_price) {
            return force_close();
        }

        let new_margin = update_margin(self.margin)?;

        let new_position = match Self::new(
            new_margin,
            self.leverage,
            self.exposure.as_running_params(),
            self.trading_fees,
            new_session_funding_fees,
            self.realized_pl,
        ) {
            Ok(position) => position,
            Err(SimulatedTradeExecutorError::CrossExposureValidation(_)) => {
                return force_close();
            }
            Err(error) => return Err(error),
        };

        if !new_position.is_coherent(market_price) {
            return force_close();
        }

        Ok((new_position, funding_fee, false))
    }

    pub fn liquidation_reached(&self, candle: &OhlcCandleRow) -> bool {
        match self.exposure {
            CrossExposure::Neutral => false,
            CrossExposure::Running(exposure) => match exposure.side() {
                TradeSide::Buy => candle.low <= exposure.liquidation().as_f64(),
                TradeSide::Sell => candle.high >= exposure.liquidation().as_f64(),
            },
        }
    }

    pub fn liquidate(&self, fee_perc: PercentageCapped) -> SimulatedTradeExecutorResult<Self> {
        if let CrossExposure::Running(exposure_running) = self.exposure {
            return self.close(exposure_running.liquidation(), fee_perc);
        }

        Ok(*self)
    }

    pub fn close(
        &self,
        close_price: Price,
        fee_perc: PercentageCapped,
    ) -> SimulatedTradeExecutorResult<Self> {
        let CrossExposure::Running(exposure) = self.exposure else {
            return Ok(*self);
        };

        let close_fee = trade_util::evaluate_order_fee(fee_perc, exposure.quantity(), close_price);
        let close_fee_i64 = i64::try_from(close_fee)
            .map_err(|_| SimulatedTradeExecutorError::CrossMarginTooHigh)?;
        let close_pl = trade_util::estimate_pl(
            exposure.side(),
            exposure.quantity(),
            exposure.entry_price(),
            close_price,
        )
        .floor() as i64;
        let margin_delta = close_pl
            .checked_sub(close_fee_i64)
            .ok_or(SimulatedTradeExecutorError::CrossMarginTooLow)?;
        let remaining_margin = Self::apply_amount_to_margin(self.margin, margin_delta)?;
        let trading_fees = self
            .trading_fees
            .checked_add(close_fee)
            .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?;

        Self::new(
            remaining_margin,
            self.leverage,
            None,
            trading_fees,
            self.session_funding_fees,
            self.realized_pl
                .checked_add(close_pl)
                .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?,
        )
    }

    pub fn with_market_order(
        &self,
        market_price: Price,
        order_side: TradeSide,
        order_quantity: CrossQuantity,
        fee_perc: PercentageCapped,
    ) -> SimulatedTradeExecutorResult<Self> {
        let curr_exposure = match self.exposure {
            CrossExposure::Neutral => None,
            CrossExposure::Running(exposure) => Some(exposure),
        };

        if let Some(curr_exposure) = curr_exposure
            && curr_exposure.side() != order_side
            && order_quantity == curr_exposure.quantity()
        {
            return self.close(market_price, fee_perc);
        }

        let order_fee = trade_util::evaluate_order_fee(fee_perc, order_quantity, market_price);
        let order_fee_i64 = i64::try_from(order_fee)
            .map_err(|_| SimulatedTradeExecutorError::CrossMarginTooHigh)?;
        let margin_after_order_fee = Self::apply_amount_to_margin(
            self.margin,
            order_fee_i64
                .checked_neg()
                .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?,
        )?;
        let cumulative_trading_fees = self
            .trading_fees
            .checked_add(order_fee)
            .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?;

        let with_running_exposure = |margin: u64,
                                     side: TradeSide,
                                     quantity: CrossQuantity,
                                     entry_price: Price,
                                     realized_pl: i64|
         -> SimulatedTradeExecutorResult<Self> {
            Self::new(
                margin,
                self.leverage,
                Some((side, quantity, entry_price)),
                cumulative_trading_fees,
                self.session_funding_fees,
                realized_pl,
            )
        };

        let Some(curr_exposure) = curr_exposure else {
            // Opening from flat uses the execution price as the new entry price.
            return with_running_exposure(
                margin_after_order_fee,
                order_side,
                order_quantity,
                market_price,
                self.realized_pl,
            );
        };

        let current_quantity = curr_exposure.quantity();
        let current_side = curr_exposure.side();
        let current_entry_price = curr_exposure.entry_price();

        if current_side == order_side {
            let resulting_quantity = current_quantity
                .try_add(order_quantity)
                .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;
            let resulting_entry_price = trade_util::aggregate_cross_entry_price(
                current_quantity,
                current_entry_price,
                order_quantity,
                market_price,
            );

            return with_running_exposure(
                margin_after_order_fee,
                order_side,
                resulting_quantity,
                resulting_entry_price,
                self.realized_pl,
            );
        }

        match order_quantity.cmp(&current_quantity) {
            Ordering::Equal => unreachable!("exact close is handled before order-fee accounting"),
            Ordering::Greater => {
                let realized_pl = trade_util::estimate_pl(
                    current_side,
                    current_quantity,
                    current_entry_price,
                    market_price,
                )
                .floor() as i64;
                let margin = Self::apply_amount_to_margin(margin_after_order_fee, realized_pl)?;
                let resulting_quantity = order_quantity
                    .try_sub(current_quantity)
                    .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;

                // Reversals book the old position P/L and start the residual side at execution price.
                let new_realized_pl = self
                    .realized_pl
                    .checked_add(realized_pl)
                    .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?;
                with_running_exposure(
                    margin,
                    order_side,
                    resulting_quantity,
                    market_price,
                    new_realized_pl,
                )
            }
            Ordering::Less => {
                let resulting_quantity = current_quantity
                    .try_sub(order_quantity)
                    .map_err(SimulatedTradeExecutorError::CrossQuantityValidation)?;
                let realized_pl = trade_util::estimate_pl(
                    current_side,
                    order_quantity,
                    current_entry_price,
                    market_price,
                )
                .floor() as i64;

                if realized_pl >= 0 {
                    let margin = Self::apply_amount_to_margin(margin_after_order_fee, realized_pl)?;

                    // Profitable partial reductions realize P/L into margin and keep entry unchanged.
                    return with_running_exposure(
                        margin,
                        current_side,
                        resulting_quantity,
                        current_entry_price,
                        self.realized_pl
                            .checked_add(realized_pl)
                            .ok_or(SimulatedTradeExecutorError::CrossMarginTooHigh)?,
                    );
                }

                // Losing partial reductions carry the loss in the remaining position entry price.
                let full_position_pl = trade_util::estimate_pl(
                    current_side,
                    current_quantity,
                    current_entry_price,
                    market_price,
                );
                let inverse_market_price = SATS_PER_BTC / market_price.as_f64();
                let carried_inverse_entry_price = match current_side {
                    TradeSide::Buy => {
                        inverse_market_price + full_position_pl / resulting_quantity.as_f64()
                    }
                    TradeSide::Sell => {
                        inverse_market_price - full_position_pl / resulting_quantity.as_f64()
                    }
                };
                let carried_entry_price =
                    Price::bounded(SATS_PER_BTC / carried_inverse_entry_price);

                with_running_exposure(
                    margin_after_order_fee,
                    current_side,
                    resulting_quantity,
                    carried_entry_price,
                    self.realized_pl,
                )
            }
        }
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

    fn realized_pl(&self) -> i64 {
        self.realized_pl
    }

    fn session_funding_fees(&self) -> i64 {
        self.session_funding_fees
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
    quantity: OrderQuantity,
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

        let Ok(new_margin) = self.margin.try_sub(funding_fee) else {
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
            Err(LeverageValidationError::NotANumber) => unreachable!("using validated types"),
        };

        let new_liquidation = trade_util::est_liquidation_from_leverage(
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
        let closing_fee = trade_util::evaluate_order_fee(fee_perc, self.quantity, close_price);

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

    fn quantity(&self) -> OrderQuantity {
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
    quantity: OrderQuantity,
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

    fn quantity(&self) -> OrderQuantity {
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
