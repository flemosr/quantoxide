use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{env::LNM_API_BASE_URL, Result};

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

const FUTURES_PRICE_HISTORY_PATH: &'static str = "/futures/history/price";

pub async fn futures_price_history(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: Option<usize>,
) -> Result<Vec<PriceEntryLNM>> {
    let mut params = Vec::new();
    if let Some(from) = from {
        params.push(("from", from.timestamp_millis().to_string()));
    }
    if let Some(to) = to {
        params.push(("to", to.timestamp_millis().to_string()));
    }
    if let Some(limit) = limit {
        params.push(("limit", limit.to_string()));
    }

    let endpoint = LNM_API_BASE_URL.clone() + FUTURES_PRICE_HISTORY_PATH;
    let url = reqwest::Url::parse_with_params(&endpoint, params)?;
    let res = reqwest::get(url).await?;

    let price_history = res.json::<Vec<PriceEntryLNM>>().await?;

    Ok(price_history)
}
