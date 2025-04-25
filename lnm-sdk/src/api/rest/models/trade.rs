use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Leverage, Margin, Price, Quantity, error::FuturesTradeRequestValidationError, utils};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSize {
    #[serde(rename = "quantity")]
    Quantity(Quantity),
    #[serde(rename = "margin")]
    Margin(Margin),
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

#[derive(Serialize)]
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

#[derive(Deserialize, Debug, Clone)]
pub struct Trade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: Margin,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    #[serde(with = "utils::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "utils::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "utils::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_ts: Option<DateTime<Utc>>,
    #[serde(with = "utils::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: u64,
}

impl Trade {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn uid(&self) -> Uuid {
        self.uid
    }

    pub fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
    }

    pub fn side(&self) -> TradeSide {
        self.side
    }

    pub fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    pub fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    pub fn maintenance_margin(&self) -> Margin {
        self.maintenance_margin
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn margin(&self) -> Margin {
        self.margin
    }

    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn liquidation(&self) -> Price {
        self.liquidation
    }

    pub fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    pub fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    pub fn exit_price(&self) -> Option<Price> {
        self.exit_price
    }

    pub fn pl(&self) -> i64 {
        self.pl
    }

    pub fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts
    }

    pub fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts
    }

    pub fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts
    }

    pub fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    pub fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn canceled(&self) -> bool {
        self.canceled
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    pub fn sum_carry_fees(&self) -> u64 {
        self.sum_carry_fees
    }
}
