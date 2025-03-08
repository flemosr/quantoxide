use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub struct LNMarketsAPI {
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
    pub fn new(lnm_api_key: String, lnm_api_secret: String, lnm_api_passphrase: String) -> Self {
        Self {
            lnm_api_key,
            lnm_api_secret,
            lnm_api_passphrase,
        }
    }

    pub async fn futures_price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<PriceEntry>, Box<dyn std::error::Error>> {
        let mut params = Vec::new();
        if let Some(from) = from {
            params.push(("from", from.timestamp_millis().to_string()));
        }
        if let Some(to) = to {
            params.push(("to", to.timestamp_millis().to_string()));
        }
        let url = reqwest::Url::parse_with_params(
            "https://api.lnmarkets.com/v2/futures/history/price",
            params,
        )?;
        let res = reqwest::get(url).await?;

        let price_history = res.json::<Vec<PriceEntry>>().await?;

        Ok(price_history)
    }
}
