use std::collections::HashMap;

use chrono::{DateTime, Utc, serde::ts_milliseconds};
use serde::Deserialize;

use super::price::Price;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    index: Price,

    last_price: Price,

    ask_price: Price,

    bid_price: Price,

    carry_fee_rate: f64,

    #[serde(with = "ts_milliseconds")]
    carry_fee_timestamp: DateTime<Utc>,

    exchanges_weights: HashMap<String, f64>,
}

impl Ticker {
    pub fn index(&self) -> Price {
        self.index
    }

    pub fn last_price(&self) -> Price {
        self.last_price
    }

    pub fn ask_price(&self) -> Price {
        self.ask_price
    }

    pub fn bid_price(&self) -> Price {
        self.bid_price
    }

    pub fn carry_fee_rate(&self) -> f64 {
        self.carry_fee_rate
    }

    pub fn carry_fee_timestamp(&self) -> DateTime<Utc> {
        self.carry_fee_timestamp
    }

    pub fn exchanges_weights(&self) -> &HashMap<String, f64> {
        &self.exchanges_weights
    }
}
