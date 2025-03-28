use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::Deserialize;

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
