use chrono::{DateTime, Utc, serde::ts_milliseconds};
use serde::Deserialize;

mod error;
mod leverage;
mod margin;
mod price;
mod quantity;
mod trade;
mod utils;

pub use leverage::Leverage;
pub use margin::Margin;
pub use price::Price;
pub use quantity::Quantity;
pub use trade::{FuturesTradeRequestBody, Trade, TradeSide, TradeType};

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
