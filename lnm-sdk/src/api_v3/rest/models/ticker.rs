use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::shared::models::price::Price;

#[derive(Debug, Clone, Deserialize)]
pub struct TickerPrice {
    ask_price: Price,
    bid_price: Price,
    min_size: u64,
    max_size: u64,
}

impl TickerPrice {
    /// Get the ask price.
    pub fn ask_price(&self) -> Price {
        self.ask_price
    }

    /// Get the bid price.
    pub fn bid_price(&self) -> Price {
        self.bid_price
    }

    /// Get the minimum size.
    pub fn min_size(&self) -> u64 {
        self.min_size
    }

    /// Get the maximum size.
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
}

/// Real-time ticker data for Bitcoin futures from LNMarkets.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    index: Price,
    last_price: Price,
    prices: Vec<TickerPrice>,
    funding_rate: f64,
    funding_time: DateTime<Utc>,
}

impl Ticker {
    /// Get the index price.
    pub fn index(&self) -> Price {
        self.index
    }

    /// Get the last price.
    pub fn last_price(&self) -> Price {
        self.last_price
    }

    /// Get the ticker prices.
    pub fn prices(&self) -> &[TickerPrice] {
        &self.prices
    }

    /// Get the funding rate.
    pub fn funding_rate(&self) -> f64 {
        self.funding_rate
    }

    /// Get the funding time.
    pub fn funding_time(&self) -> DateTime<Utc> {
        self.funding_time
    }
}
