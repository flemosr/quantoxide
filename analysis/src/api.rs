use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug)]
pub struct LNMarketsAPI {
    lnm_api_base_url: String,
    lnm_api_key: String,
    lnm_api_secret: String,
    lnm_api_passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct PriceEntry {
    #[serde(with = "ts_milliseconds")]
    time: DateTime<Utc>,
    value: f64,
}

impl PriceEntry {
    pub fn time(&self) -> &DateTime<Utc> {
        &self.time
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

impl LNMarketsAPI {
    pub fn new(
        lnm_api_base_url: String,
        lnm_api_key: String,
        lnm_api_secret: String,
        lnm_api_passphrase: String,
    ) -> Self {
        Self {
            lnm_api_base_url,
            lnm_api_key,
            lnm_api_secret,
            lnm_api_passphrase,
        }
    }

    const FUTURES_PRICE_HISTORY_PATH: &'static str = "/futures/history/price";

    pub async fn futures_price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<u32>,
    ) -> Result<Vec<PriceEntry>, Box<dyn std::error::Error>> {
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

        let endpoint = self.lnm_api_base_url.clone() + Self::FUTURES_PRICE_HISTORY_PATH;
        let url = reqwest::Url::parse_with_params(&endpoint, params)?;
        let res = reqwest::get(url).await?;

        let price_history = res.json::<Vec<PriceEntry>>().await?;

        Ok(price_history)
    }
}
