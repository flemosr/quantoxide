use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
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

mod option_float_without_decimal {
    use super::float_without_decimal;
    use serde::{self, Serializer};

    pub fn serialize<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => float_without_decimal::serialize(v, serializer),
            None => serializer.serialize_none(),
        }
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
    #[serde(with = "float_without_decimal")]
    pub price: f64,
    #[serde(
        with = "option_float_without_decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub stoploss: Option<f64>,
    #[serde(
        with = "option_float_without_decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub takeprofit: Option<f64>,
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
