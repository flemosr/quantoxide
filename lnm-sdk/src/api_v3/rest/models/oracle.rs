use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::shared::models::price::Price;

#[derive(Deserialize, Debug, Clone)]
pub struct Index {
    time: DateTime<Utc>,
    index: Price,
}

impl Index {
    /// Timestamp of the index data point.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Index price value.
    pub fn index(&self) -> Price {
        self.index
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LastPrice {
    time: DateTime<Utc>,
    last_price: Price,
}

impl LastPrice {
    /// Timestamp of the last price data point.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Last price value.
    pub fn last_price(&self) -> Price {
        self.last_price
    }
}
