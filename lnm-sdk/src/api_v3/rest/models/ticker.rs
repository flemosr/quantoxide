use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::shared::models::price::Price;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
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

    pub fn as_data_str(&self) -> String {
        format!(
            "ask_price: {}\nbid_price: {}\nmin_size: {}\nmax_size: {}",
            self.ask_price, self.bid_price, self.min_size, self.max_size
        )
    }
}

impl fmt::Display for TickerPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ticker Price:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}

/// Real-time ticker data for Bitcoin futures from LN Markets.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::Ticker;
///
/// let ticker: Ticker = rest
///     .futures_data
///     .get_ticker()
///     .await?;
///
/// println!("Index: {}", ticker.index());
/// println!("Last price: {}", ticker.last_price());
/// println!("Funding rate: {}", ticker.funding_rate());
/// println!("Funding time: {}", ticker.funding_time());
///
/// for price in ticker.prices() {
///     println!("Ask: {}, Bid: {}", price.ask_price(), price.bid_price());
/// }
/// # Ok(())
/// # }
/// ```
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

    pub fn as_data_str(&self) -> String {
        format!(
            "index: {}\nlast_price: {}\nfunding_rate: {:.6}\nfunding_time: {}",
            self.index,
            self.last_price,
            self.funding_rate,
            self.funding_time.to_rfc3339()
        )
    }
}

impl fmt::Display for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ticker:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
