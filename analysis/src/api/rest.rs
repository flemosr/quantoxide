use chrono::{DateTime, Utc};

use super::error::{ApiError, Result};

pub mod models;

use models::PriceEntryLNM;

const FUTURES_PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";

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

    let url = super::get_endpoint_url(FUTURES_PRICE_HISTORY_PATH, Some(params))?;
    let res = reqwest::get(url).await.map_err(ApiError::Response)?;

    let price_history = res
        .json::<Vec<PriceEntryLNM>>()
        .await
        .map_err(ApiError::UnexpectedSchema)?;

    Ok(price_history)
}
