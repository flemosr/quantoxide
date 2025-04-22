use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, convert::TryFrom};
use uuid::Uuid;

mod error;
mod leverage;
mod price;
mod utils;

use error::{FuturesTradeRequestValidationError, MarginValidationError, QuantityValidationError};

pub use leverage::Leverage;
pub use price::Price;

#[derive(Debug, Deserialize)]
pub struct PriceEntryLNM {
    #[serde(with = "ts_milliseconds")]
    time: DateTime<Utc>,
    value: f64,
}

impl PriceEntryLNM {
    pub fn time(&self) -> &DateTime<Utc> {
        &self.time
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Quantity(u64);

impl Quantity {
    pub fn to_u64(&self) -> u64 {
        self.0
    }

    pub fn try_calculate(
        margin: Margin,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.to_u64() as f64 / 100000000. * price.to_f64() * leverage.to_f64();
        Self::try_from(qtd)
    }
}

impl TryFrom<u64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: u64) -> Result<Self, Self::Error> {
        if quantity < 1 {
            return Err(QuantityValidationError::TooLow);
        }

        if quantity > 500_000 {
            return Err(QuantityValidationError::TooHigh);
        }

        Ok(Quantity(quantity))
    }
}

impl TryFrom<i32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: i32) -> Result<Self, Self::Error> {
        if quantity < 0 {
            return Err(QuantityValidationError::NotPositive);
        }

        Self::try_from(quantity as u64)
    }
}

impl TryFrom<f64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: f64) -> Result<Self, Self::Error> {
        if !quantity.is_finite() {
            return Err(QuantityValidationError::NotFinite);
        }

        if quantity < 0. {
            return Err(QuantityValidationError::NotPositive);
        }

        let quantity_u64 = quantity as u64;

        Self::try_from(quantity_u64)
    }
}

impl PartialEq for Quantity {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Quantity {}

impl PartialOrd for Quantity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Quantity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq<u64> for Quantity {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u64> for Quantity {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialEq<Quantity> for u64 {
    fn eq(&self, other: &Quantity) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Quantity> for u64 {
    fn partial_cmp(&self, other: &Quantity) -> Option<Ordering> {
        Some(self.cmp(&other.0))
    }
}

impl PartialEq<i32> for Quantity {
    fn eq(&self, other: &i32) -> bool {
        if *other < 0 {
            false
        } else {
            self.0 == *other as u64
        }
    }
}

impl PartialOrd<i32> for Quantity {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        if *other < 0 {
            Some(Ordering::Greater)
        } else {
            Some(self.0.cmp(&(*other as u64)))
        }
    }
}

impl PartialEq<Quantity> for i32 {
    fn eq(&self, other: &Quantity) -> bool {
        if *self < 0 {
            false
        } else {
            *self as u64 == other.0
        }
    }
}

impl PartialOrd<Quantity> for i32 {
    fn partial_cmp(&self, other: &Quantity) -> Option<Ordering> {
        if *self < 0 {
            Some(Ordering::Less)
        } else {
            Some((*self as u64).cmp(&other.0))
        }
    }
}

impl Serialize for Quantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Margin(u64);

impl Margin {
    pub fn to_u64(&self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: u64) -> Result<Self, Self::Error> {
        if margin < 1 {
            return Err(MarginValidationError::TooLow);
        }

        Ok(Margin(margin))
    }
}

impl TryFrom<i32> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: i32) -> Result<Self, Self::Error> {
        if margin < 0 {
            return Err(MarginValidationError::NotPositive);
        }

        Self::try_from(margin as u64)
    }
}

impl TryFrom<f64> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: f64) -> Result<Self, Self::Error> {
        if !margin.is_finite() {
            return Err(MarginValidationError::NotFinite);
        }

        if margin < 0. {
            return Err(MarginValidationError::NotPositive);
        }

        Ok(Margin(margin as u64))
    }
}

impl PartialEq for Margin {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Margin {}

impl PartialOrd for Margin {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Margin {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq<u64> for Margin {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u64> for Margin {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialEq<Margin> for u64 {
    fn eq(&self, other: &Margin) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Margin> for u64 {
    fn partial_cmp(&self, other: &Margin) -> Option<Ordering> {
        Some(self.cmp(&other.0))
    }
}

impl PartialEq<i32> for Margin {
    fn eq(&self, other: &i32) -> bool {
        if *other < 0 {
            false
        } else {
            self.0 == *other as u64
        }
    }
}

impl PartialOrd<i32> for Margin {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        if *other < 0 {
            Some(Ordering::Greater)
        } else {
            Some(self.0.cmp(&(*other as u64)))
        }
    }
}

impl PartialEq<Margin> for i32 {
    fn eq(&self, other: &Margin) -> bool {
        if *self < 0 {
            false
        } else {
            *self as u64 == other.0
        }
    }
}

impl PartialOrd<Margin> for i32 {
    fn partial_cmp(&self, other: &Margin) -> Option<Ordering> {
        if *self < 0 {
            Some(Ordering::Less)
        } else {
            Some((*self as u64).cmp(&other.0))
        }
    }
}

impl Serialize for Margin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
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

    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<Quantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin: Option<Margin>,

    #[serde(rename = "type")]
    trade_type: TradeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        quantity: Option<Quantity>,
        margin: Option<Margin>,
        trade_type: TradeType,
        price: Option<Price>,
    ) -> Result<Self, FuturesTradeRequestValidationError> {
        match (quantity, margin) {
            (None, None) => {
                return Err(FuturesTradeRequestValidationError::MissingQuantityAndMargin);
            }
            (Some(_), Some(_)) => {
                return Err(FuturesTradeRequestValidationError::BothQuantityAndMarginProvided);
            }
            _ => {}
        }

        match (&trade_type, price) {
            (TradeType::Market, Some(_)) => {
                return Err(FuturesTradeRequestValidationError::PriceSetForMarketOrder);
            }
            (TradeType::Limit, None) => {
                return Err(FuturesTradeRequestValidationError::MissingPriceForLimitOrder);
            }
            _ => {}
        }

        match (margin, price) {
            (Some(margin), Some(price)) => {
                let _ = Quantity::try_calculate(margin, price, leverage)?;
            }
            _ => {}
        };

        if let Some(price_val) = price {
            if let Some(stoploss_val) = stoploss {
                if stoploss_val >= price_val {
                    return Err(FuturesTradeRequestValidationError::StopLossHigherThanPrice);
                }
            }

            if let Some(takeprofit_val) = takeprofit {
                if takeprofit_val <= price_val {
                    return Err(FuturesTradeRequestValidationError::TakeProfitLowerThanPrice);
                }
            }
        }

        Ok(FuturesTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            quantity,
            margin,
            trade_type,
            price,
        })
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Trade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeType,
    side: TradeSide,
    opening_fee: i64,
    closing_fee: i64,
    maintenance_margin: i64,
    quantity: f64,
    margin: i64,
    leverage: f64,
    price: f64,
    liquidation: f64,
    stoploss: f64,
    takeprofit: f64,
    exit_price: Option<f64>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    closed_ts: Option<String>,
    entry_price: Option<f64>,
    entry_margin: Option<i64>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TradeType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}
