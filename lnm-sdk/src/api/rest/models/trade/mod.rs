use std::{
    fmt::{self},
    result::Result,
};

use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};

pub use uuid::Uuid;

use super::{
    Leverage, Margin, Price, Quantity,
    error::{FuturesTradeRequestValidationError, TradeValidationError, ValidationError},
    serde_util,
};

pub mod util;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

impl fmt::Display for TradeSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSide::Buy => "Buy".fmt(f),
            TradeSide::Sell => "Sell".fmt(f),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSize {
    #[serde(rename = "quantity")]
    Quantity(Quantity),
    #[serde(rename = "margin")]
    Margin(Margin),
}

impl TradeSize {
    pub fn to_quantity_and_margin(
        &self,
        price: Price,
        leverage: Leverage,
    ) -> Result<(Quantity, Margin), ValidationError> {
        match self {
            TradeSize::Margin(margin) => {
                let quantity = Quantity::try_calculate(*margin, price, leverage)?;
                Ok((quantity, *margin))
            }
            TradeSize::Quantity(quantity) => {
                let margin = Margin::calculate(*quantity, price, leverage);
                Ok((*quantity, margin))
            }
        }
    }
}

impl From<Quantity> for TradeSize {
    fn from(quantity: Quantity) -> Self {
        TradeSize::Quantity(quantity)
    }
}

impl From<Margin> for TradeSize {
    fn from(margin: Margin) -> Self {
        TradeSize::Margin(margin)
    }
}

impl fmt::Display for TradeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSize::Quantity(quantity) => write!(f, "Quantity({})", quantity),
            TradeSize::Margin(margin) => write!(f, "Margin({})", margin),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecutionType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecution {
    Market,
    Limit(Price),
}

impl TradeExecution {
    pub fn to_type(&self) -> TradeExecutionType {
        match self {
            TradeExecution::Market => TradeExecutionType::Market,
            TradeExecution::Limit(_) => TradeExecutionType::Limit,
        }
    }
}

impl From<Price> for TradeExecution {
    fn from(price: Price) -> Self {
        Self::Limit(price)
    }
}

#[derive(Serialize, Debug)]
pub struct FuturesTradeRequestBody {
    leverage: Leverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    side: TradeSide,

    #[serde(flatten)]
    size: TradeSize,

    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss {
                if stoploss >= price {
                    return Err(FuturesTradeRequestValidationError::StopLossHigherThanPrice);
                }
            }

            if let Some(takeprofit) = takeprofit {
                if takeprofit <= price {
                    return Err(FuturesTradeRequestValidationError::TakeProfitLowerThanPrice);
                }
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        Ok(FuturesTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            size,
            trade_type,
            price,
        })
    }
}

pub enum TradeStatus {
    Open,
    Running,
    Closed,
}

impl TradeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeStatus::Open => "open",
            TradeStatus::Running => "running",
            TradeStatus::Closed => "closed",
        }
    }

    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

pub trait Trade: Send + Sync + fmt::Debug + 'static {
    fn id(&self) -> Uuid;
    fn trade_type(&self) -> TradeExecutionType;
    fn side(&self) -> TradeSide;
    fn opening_fee(&self) -> u64;
    fn closing_fee(&self) -> u64;
    fn maintenance_margin(&self) -> i64;
    fn quantity(&self) -> Quantity;
    fn margin(&self) -> Margin;
    fn leverage(&self) -> Leverage;
    fn price(&self) -> Price;
    fn liquidation(&self) -> Price;
    fn stoploss(&self) -> Option<Price>;
    fn takeprofit(&self) -> Option<Price>;
    fn exit_price(&self) -> Option<Price>;
    fn creation_ts(&self) -> DateTime<Utc>;
    fn market_filled_ts(&self) -> Option<DateTime<Utc>>;
    fn closed_ts(&self) -> Option<DateTime<Utc>>;
    fn entry_price(&self) -> Option<Price>;
    fn entry_margin(&self) -> Option<Margin>;
    fn open(&self) -> bool;
    fn running(&self) -> bool;
    fn canceled(&self) -> bool;
    fn closed(&self) -> bool;
}

pub trait TradeRunning: Trade {
    fn est_pl(&self, market_price: Price) -> f64;

    fn est_max_additional_margin(&self) -> u64 {
        if self.leverage() == Leverage::MIN {
            return 0;
        }

        let max_margin = Margin::calculate(self.quantity(), self.price(), Leverage::MIN);

        let max_add_margin = max_margin
            .into_u64()
            .saturating_sub(self.margin().into_u64());

        return max_add_margin;
    }

    fn est_max_cash_in(&self, market_price: Price) -> u64 {
        let extractable_pl = self.est_pl(market_price).max(0.) as u64;

        let min_margin = Margin::calculate(self.quantity(), self.price(), Leverage::MAX);

        let max_cash_in = self
            .margin()
            .into_u64()
            .saturating_sub(min_margin.into_u64())
            + extractable_pl;

        return max_cash_in;
    }

    fn est_collateral_delta_for_liquidation(
        &self,
        target_liquidation: Price,
        market_price: Price,
    ) -> Result<i64, TradeValidationError> {
        util::evaluate_collateral_delta_for_liquidation(
            self.side(),
            self.quantity(),
            self.margin(),
            self.price(),
            self.liquidation(),
            target_liquidation,
            market_price,
        )
    }
}

pub trait TradeClosed: Trade {
    fn pl(&self) -> i64;
}

#[derive(Deserialize, Debug, Clone)]
pub struct LnmTrade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: i64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    #[serde(with = "serde_util::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_ts: Option<DateTime<Utc>>,
    #[serde(with = "serde_util::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: i64,
}

impl LnmTrade {
    pub fn uid(&self) -> Uuid {
        self.uid
    }

    pub fn pl(&self) -> i64 {
        self.pl
    }

    pub fn sum_carry_fees(&self) -> i64 {
        self.sum_carry_fees
    }
}

impl Trade for LnmTrade {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
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
        self.maintenance_margin
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
        self.exit_price
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts
    }

    fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    fn open(&self) -> bool {
        self.open
    }

    fn running(&self) -> bool {
        self.running
    }

    fn canceled(&self) -> bool {
        self.canceled
    }

    fn closed(&self) -> bool {
        self.closed
    }
}

impl TradeRunning for LnmTrade {
    fn est_pl(&self, market_price: Price) -> f64 {
        util::estimate_pl(self.side(), self.quantity(), self.price(), market_price)
    }
}

impl TradeClosed for LnmTrade {
    fn pl(&self) -> i64 {
        self.pl
    }
}

#[derive(Deserialize)]
pub struct NestedTradesResponse {
    pub trades: Vec<LnmTrade>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TradeUpdateType {
    Stoploss,
    Takeprofit,
}

impl TradeUpdateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeUpdateType::Stoploss => "stoploss",
            TradeUpdateType::Takeprofit => "takeprofit",
        }
    }
}

#[derive(Serialize, Debug)]
pub struct FuturesUpdateTradeRequestBody {
    id: Uuid,
    #[serde(rename = "type")]
    update_type: TradeUpdateType,
    value: Price,
}

impl FuturesUpdateTradeRequestBody {
    pub fn new(id: Uuid, update_type: TradeUpdateType, value: Price) -> Self {
        Self {
            id,
            update_type,
            value,
        }
    }
}
