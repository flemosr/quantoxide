use std::{num::NonZeroU64, sync::Arc};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api_v3::models::{
    Leverage, Margin, PercentageCapped, Price, Quantity, TradeSide, TradeSize, trade_util,
};

use super::{
    super::super::core::{TradeClosed, TradeCore, TradeRunning},
    error::{SimulatedTradeExecutorError, SimulatedTradeExecutorResult},
};

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
        }))
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
}

impl crate::sealed::Sealed for SimulatedTradeClosed {}

impl TradeClosed for SimulatedTradeClosed {
    fn pl(&self) -> i64 {
        trade_util::estimate_pl(self.side(), self.quantity(), self.price(), self.close_price)
            .floor() as i64
    }
}
