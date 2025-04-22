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

impl FuturePrice {
    pub fn to_f64(&self) -> f64 {
        self.0
    }
}

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

#[derive(Debug, Clone, Copy)]
pub struct Leverage(f64);

impl Leverage {
    pub fn to_f64(&self) -> f64 {
        self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LeverageValidationError {
    #[error("Leverage must be at least 1")]
    TooLow,

    #[error("Leverage must be at most 100")]
    TooHigh,

    #[error("Leverage must be a finite number")]
    NotFinite,
}

impl TryFrom<f64> for Leverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: f64) -> Result<Self, Self::Error> {
        if !leverage.is_finite() {
            return Err(LeverageValidationError::NotFinite);
        }

        if leverage < 1.0 {
            return Err(LeverageValidationError::TooLow);
        }

        if leverage > 100.0 {
            return Err(LeverageValidationError::TooHigh);
        }

        Ok(Leverage(leverage))
    }
}

impl TryFrom<i32> for Leverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: i32) -> Result<Self, Self::Error> {
        Self::try_from(leverage as f64)
    }
}

impl PartialEq for Leverage {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Leverage {}

impl PartialOrd for Leverage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for Leverage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq<f64> for Leverage {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for Leverage {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<Leverage> for f64 {
    fn eq(&self, other: &Leverage) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Leverage> for f64 {
    fn partial_cmp(&self, other: &Leverage) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialEq<i32> for Leverage {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<i32> for Leverage {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<Leverage> for i32 {
    fn eq(&self, other: &Leverage) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<Leverage> for i32 {
    fn partial_cmp(&self, other: &Leverage) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
    }
}

impl Serialize for Leverage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        float_without_decimal::serialize(&self.0, serializer)
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
        price: FuturePrice,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.to_u64() as f64 / 100000000. * price.to_f64() * leverage.to_f64();
        Self::try_from(qtd)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QuantityValidationError {
    #[error("Quantity must be positive")]
    NotPositive,

    #[error("Quantity must be at least 1")]
    TooLow,

    #[error("Quantity must be less than or equal to 500,000")]
    TooHigh,

    #[error("Quantity must be a finite number")]
    NotFinite,
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

#[derive(Debug, thiserror::Error)]
pub enum MarginValidationError {
    #[error("Margin must be positive")]
    NotPositive,

    #[error("Margin must be at least 1")]
    TooLow,

    // #[error("Margin must be less than or equal to 1,000,000,000")]
    // TooHigh,
    #[error("Margin must be a finite number")]
    NotFinite,
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

#[derive(Debug, thiserror::Error)]
pub enum FuturesTradeRequestValidationError {
    #[error("Either quantity or margin must be provided")]
    MissingQuantityAndMargin,

    #[error("Cannot provide both quantity and margin")]
    BothQuantityAndMarginProvided,

    #[error("Price cannot be set for market orders")]
    PriceSetForMarketOrder,

    #[error("Price must be set for limit orders")]
    MissingPriceForLimitOrder,

    #[error("Implied quantity must be valid")]
    InvalidImpliedQuantity(#[from] QuantityValidationError),
}

#[derive(Serialize)]
pub struct FuturesTradeRequestBody {
    leverage: Leverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<FuturePrice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<FuturePrice>,
    side: TradeSide,

    quantity: Option<Quantity>,
    margin: Option<Margin>,

    #[serde(rename = "type")]
    trade_type: TradeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<FuturePrice>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<FuturePrice>,
        takeprofit: Option<FuturePrice>,
        side: TradeSide,
        quantity: Option<Quantity>,
        margin: Option<Margin>,
        trade_type: TradeType,
        price: Option<FuturePrice>,
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
