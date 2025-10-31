use chrono::{DateTime, Utc, serde::ts_milliseconds};
use serde::Deserialize;

use super::price::Price;

#[derive(Debug, Deserialize)]
pub struct PriceEntryLNM {
    #[serde(with = "ts_milliseconds")]
    time: DateTime<Utc>,
    value: Price,
}

impl PriceEntryLNM {
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn value(&self) -> Price {
        self.value
    }
}
