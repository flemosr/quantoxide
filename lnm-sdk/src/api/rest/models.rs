use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, convert::TryFrom};
use uuid::Uuid;

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

mod float_without_decimal {
    use serde::{self, Serializer};

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if value.fract() == 0.0 {
            serializer.serialize_i64(*value as i64)
        } else {
            serializer.serialize_f64(*value)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FuturePrice(f64);

#[derive(Debug, thiserror::Error)]
pub enum PriceValidationError {
    #[error("Price must be positive")]
    NotPositive,

    #[error("Price must be a multiple of 0.5")]
    NotMultipleOfTick,

    #[error("Price must be a finite number")]
    NotFinite,
}

impl TryFrom<f64> for FuturePrice {
    type Error = PriceValidationError;

    fn try_from(price: f64) -> Result<Self, Self::Error> {
        if !price.is_finite() {
            return Err(PriceValidationError::NotFinite);
        }

        if price <= 0.0 {
            return Err(PriceValidationError::NotPositive);
        }

        if (price * 2.0).round() != price * 2.0 {
            return Err(PriceValidationError::NotMultipleOfTick);
        }

        Ok(FuturePrice(price))
    }
}

impl TryFrom<i32> for FuturePrice {
    type Error = PriceValidationError;

    fn try_from(price: i32) -> Result<Self, Self::Error> {
        Self::try_from(price as f64)
    }
}

impl FuturePrice {
    pub fn to_f64(&self) -> f64 {
        self.0
    }
}

impl PartialEq for FuturePrice {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for FuturePrice {}

impl PartialOrd for FuturePrice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for FuturePrice {
    fn cmp(&self, other: &Self) -> Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq<f64> for FuturePrice {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for FuturePrice {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<FuturePrice> for f64 {
    fn eq(&self, other: &FuturePrice) -> bool {
        *self == other.0
    }
}

impl PartialOrd<FuturePrice> for f64 {
    fn partial_cmp(&self, other: &FuturePrice) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialEq<i32> for FuturePrice {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<i32> for FuturePrice {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<FuturePrice> for i32 {
    fn eq(&self, other: &FuturePrice) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<FuturePrice> for i32 {
    fn partial_cmp(&self, other: &FuturePrice) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
    }
}

impl PartialEq<u32> for FuturePrice {
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<u32> for FuturePrice {
    fn partial_cmp(&self, other: &u32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<FuturePrice> for u32 {
    fn eq(&self, other: &FuturePrice) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<FuturePrice> for u32 {
    fn partial_cmp(&self, other: &FuturePrice) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
    }
}

impl Serialize for FuturePrice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        float_without_decimal::serialize(&self.0, serializer)
    }
}

#[derive(Serialize)]
pub struct FuturesTradeRequestBody {
    pub side: TradeSide,
    pub margin: u64,
    #[serde(with = "float_without_decimal")]
    pub leverage: f64,
    #[serde(rename = "type")]
    pub trade_type: TradeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<FuturePrice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stoploss: Option<FuturePrice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeprofit: Option<FuturePrice>,
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
