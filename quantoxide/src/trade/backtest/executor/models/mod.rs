use std::{num::NonZeroU64, sync::Arc};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, Margin, Price, Quantity, SATS_PER_BTC, Trade, TradeClosed,
    TradeExecutionType, TradeRunning, TradeSide, TradeSize, trade_util,
};

use super::super::super::{
    core::TradeExt,
    error::{Result, TradeError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeRunning {
    id: Uuid,
    side: TradeSide,
    opening_fee: u64,
    closing_fee_reserved: u64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    stoploss: Price,
    takeprofit: Price,
    entry_time: DateTime<Utc>,
    entry_price: Price,
}

impl SimulatedTradeRunning {
    pub fn new(
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        entry_time: DateTime<Utc>,
        entry_price: Price,
        stoploss: Price,
        takeprofit: Price,
        fee_perc: BoundedPercentage,
    ) -> Result<Arc<Self>> {
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
            .map_err(TradeError::TradeValidation)?;

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
            entry_price: entry_price,
        }))
    }

    pub fn with_new_stoploss(&self, market_price: Price, new_stoploss: Price) -> Result<Arc<Self>> {
        trade_util::evaluate_new_stoploss(
            self.side,
            self.liquidation,
            Some(self.takeprofit),
            market_price,
            new_stoploss,
        )
        .map_err(TradeError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            stoploss: new_stoploss,
            takeprofit: self.takeprofit,
            margin: self.margin,
            quantity: self.quantity,
            leverage: self.leverage,
            liquidation: self.liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
        }))
    }

    pub fn with_added_margin(&self, amount: NonZeroU64) -> Result<Arc<Self>> {
        let (new_margin, new_leverage, new_liquidation) = trade_util::evaluate_added_margin(
            self.side,
            self.quantity,
            self.price,
            self.margin,
            amount,
        )
        .map_err(TradeError::TradeValidation)?;

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

    pub fn with_cash_in(&self, market_price: Price, amount: NonZeroU64) -> Result<Arc<Self>> {
        let (new_price, new_margin, new_leverage, new_liquidation, new_stoploss) =
            trade_util::evaluate_cash_in(
                self.side,
                self.quantity,
                self.margin,
                self.price,
                Some(self.stoploss),
                market_price,
                amount,
            )
            .map_err(TradeError::TradeValidation)?;

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: new_price,
            // TODO: Fix once `stoploss` is optional
            stoploss: new_stoploss.unwrap_or(new_liquidation),
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
        close_time: DateTime<Utc>,
        close_price: Price,
        fee_perc: BoundedPercentage,
    ) -> Arc<SimulatedTradeClosed> {
        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let closing_fee =
            (fee_calc * self.quantity.into_f64() / close_price.into_f64()).floor() as u64;

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

impl Trade for SimulatedTradeRunning {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        TradeExecutionType::Market
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
        Some(self.stoploss)
    }

    fn takeprofit(&self) -> Option<Price> {
        Some(self.takeprofit)
    }

    fn exit_price(&self) -> Option<Price> {
        None
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn entry_price(&self) -> Option<Price> {
        Some(self.entry_price)
    }

    fn entry_margin(&self) -> Option<Margin> {
        Some(self.margin)
    }

    fn open(&self) -> bool {
        false
    }

    fn running(&self) -> bool {
        true
    }

    fn canceled(&self) -> bool {
        false
    }

    fn closed(&self) -> bool {
        false
    }
}

impl TradeRunning for SimulatedTradeRunning {
    fn est_pl(&self, market_price: Price) -> i64 {
        trade_util::pl_estimate(self.side(), self.quantity(), self.price(), market_price)
    }
}

impl TradeExt for SimulatedTradeRunning {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeClosed {
    id: Uuid,
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    price: Price,
    liquidation: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    quantity: Quantity,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
    opening_fee: u64,
    closing_fee_reserved: u64,
    closing_fee: u64,
}

impl Trade for SimulatedTradeClosed {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        TradeExecutionType::Market
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
        Some(self.stoploss)
    }

    fn takeprofit(&self) -> Option<Price> {
        Some(self.takeprofit)
    }

    fn exit_price(&self) -> Option<Price> {
        Some(self.close_price)
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.close_time)
    }

    fn entry_price(&self) -> Option<Price> {
        Some(self.entry_price)
    }

    fn entry_margin(&self) -> Option<Margin> {
        Some(self.margin)
    }

    fn open(&self) -> bool {
        false
    }

    fn running(&self) -> bool {
        false
    }

    fn canceled(&self) -> bool {
        false
    }

    fn closed(&self) -> bool {
        true
    }
}

impl TradeClosed for SimulatedTradeClosed {
    fn pl(&self) -> i64 {
        trade_util::pl_estimate(self.side(), self.quantity(), self.price(), self.close_price)
    }
}
