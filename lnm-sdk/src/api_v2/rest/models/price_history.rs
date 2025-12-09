use chrono::{DateTime, Utc, serde::ts_milliseconds};
use serde::Deserialize;

use crate::shared::models::price::Price;

/// A historical price entry from the LN Markets futures API.
///
/// This type represents a single price observation at a specific point in time,
/// as returned by the price history endpoint. Each entry contains a timestamp
/// and the corresponding Bitcoin price in USD.
///
/// # Examples
///
/// ```no_run
/// # #[allow(deprecated)]
/// # async fn example(rest_api: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// # use lnm_sdk::api_v2::models::PriceEntry;
/// let price_history: Vec<PriceEntry> = rest_api
///     .futures
///     .price_history(None, None, None)
///     .await?;
///
/// for entry in price_history {
///     println!("Price at {}: {}", entry.time(), entry.value());
/// }
/// # Ok(())
/// # }
/// ```
///
/// [`futures.price_history`]: crate::api::rest::repositories::FuturesRepository::price_history
#[derive(Debug, Deserialize)]
pub struct PriceEntry {
    #[serde(with = "ts_milliseconds")]
    time: DateTime<Utc>,
    value: Price,
}

impl PriceEntry {
    /// Returns the timestamp when this price was observed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest_api: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::models::PriceEntry;
    /// let price_history: Vec<PriceEntry> = rest_api
    ///     .futures
    ///     .price_history(None, None, None)
    ///     .await?;
    ///
    /// if let Some(entry) = price_history.first() {
    ///     let timestamp = entry.time();
    ///     println!("Price observed at: {}", timestamp);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Returns the Bitcoin price in USD for this entry.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest_api: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::models::PriceEntry;
    /// let price_history: Vec<PriceEntry> = rest_api
    ///     .futures
    ///     .price_history(None, None, None)
    ///     .await?;
    ///
    /// if let Some(entry) = price_history.first() {
    ///     let price = entry.value();
    ///     println!("Bitcoin price: ${}", price.as_f64());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn value(&self) -> Price {
        self.value
    }
}
