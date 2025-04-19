use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::Deserialize;
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

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TradeType {
    M, // Market order
    L, // Limit order
}

impl TradeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeType::M => "m",
            TradeType::L => "l",
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TradeSide {
    B, // Buy
    S, // Sell
}

impl TradeSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeSide::B => "b",
            TradeSide::S => "s",
        }
    }
}
