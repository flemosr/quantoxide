use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc, serde::ts_milliseconds};
use serde::Deserialize;

use crate::shared::models::price::Price;

/// Real-time ticker data for Bitcoin futures from LN Markets.
///
/// Contains current market data including index price, last traded price, bid/ask prices,
/// carry fees, and the weights of different exchanges contributing to the index calculation.
///
/// # Examples
///
/// ```no_run
/// # #[allow(deprecated)]
/// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v2::models::Ticker;
///
/// let ticker: Ticker = rest.futures.ticker().await?;
///
/// println!("Index: {}", ticker.index());
/// println!("Last price: {}", ticker.last_price());
/// println!("Spread: {} - {}", ticker.bid_price(), ticker.ask_price());
/// # Ok(())
/// # }
/// ```
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
    /// Returns the index price.
    ///
    /// The index price is a weighted average of Bitcoin prices across multiple exchanges, used as
    /// the reference price for futures contracts.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Index price: ${}", ticker.index().as_f64());
    /// # Ok(())
    /// # }
    /// ```
    pub fn index(&self) -> Price {
        self.index
    }

    /// Returns the last traded price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Last trade: ${}", ticker.last_price().as_f64());
    /// # Ok(())
    /// # }
    /// ```
    pub fn last_price(&self) -> Price {
        self.last_price
    }

    /// Returns the current ask price (lowest sell order).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Ask: ${}", ticker.ask_price().as_f64());
    /// # Ok(())
    /// # }
    /// ```
    pub fn ask_price(&self) -> Price {
        self.ask_price
    }

    /// Returns the current bid price (highest buy order).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Bid: ${}", ticker.bid_price().as_f64());
    /// # Ok(())
    /// # }
    /// ```
    pub fn bid_price(&self) -> Price {
        self.bid_price
    }

    /// Returns the carry fee rate.
    ///
    /// The carry fee is applied to open positions and is based on the funding rate mechanism.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Carry fee rate: {:.6}%", ticker.carry_fee_rate() * 100.0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn carry_fee_rate(&self) -> f64 {
        self.carry_fee_rate
    }

    /// Returns the timestamp when the carry fee will be settled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// println!("Carry fee settlement at: {}", ticker.carry_fee_timestamp());
    /// # Ok(())
    /// # }
    /// ```
    pub fn carry_fee_timestamp(&self) -> DateTime<Utc> {
        self.carry_fee_timestamp
    }

    /// Returns the weights of different exchanges used in the index calculation.
    ///
    /// Each entry maps an exchange name to its weight (as a value between 0.0 and 1.0)
    /// in the index price calculation.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ticker = rest.futures.ticker().await?;
    ///
    /// for (exchange, weight) in ticker.exchanges_weights() {
    ///     println!("{}: {:.2}%", exchange, weight * 100.0);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn exchanges_weights(&self) -> &HashMap<String, f64> {
        &self.exchanges_weights
    }

    pub fn as_data_str(&self) -> String {
        format!(
            "index: {}\nlast_price: {}\nbid_price: {}\nask_price: {}\ncarry_fee_rate: {:.6}\ncarry_fee_timestamp: {}",
            self.index,
            self.last_price,
            self.bid_price,
            self.ask_price,
            self.carry_fee_rate,
            self.carry_fee_timestamp.to_rfc3339()
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
