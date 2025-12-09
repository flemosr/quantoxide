use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::shared::models::price::Price;

/// Index price data point.
///
/// Represents the index price at a specific point in time.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::Index;
///
/// let index_history: Vec<Index> = rest_api
///     .oracle
///     .get_index(None, None, None, None)
///     .await?;
///
/// for index in index_history {
///     println!("Time: {}", index.time());
///     println!("Index: {}", index.index());
/// }
/// # Ok(())
/// # }
/// ```
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

/// Last traded price data point.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::LastPrice;
///
/// let price_history: Vec<LastPrice> = rest_api
///     .oracle
///     .get_last_price(None, None, None, None)
///     .await?;
///
/// for price in price_history {
///     println!("Time: {}", price.time());
///     println!("Last price: {}", price.last_price());
/// }
/// # Ok(())
/// # }
/// ```
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
