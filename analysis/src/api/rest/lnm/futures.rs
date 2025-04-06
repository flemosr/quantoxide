use std::borrow::Borrow;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Url;

use super::super::super::error::{ApiError, Result};
use super::super::{models::PriceEntryLNM, repositories::FuturesRepository};

const PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";

pub struct LnmFuturesRepository {
    api_domain: String,
}

impl LnmFuturesRepository {
    pub fn new(api_domain: String) -> Self {
        Self { api_domain }
    }

    fn api_domain(&self) -> &String {
        &self.api_domain
    }

    fn get_endpoint_url<I, K, V>(&self, path: impl AsRef<str>, params: Option<I>) -> Result<Url>
    where
        I: IntoIterator,
        I::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let base_endpoint_url = format!("https://{}{}", self.api_domain(), path.as_ref());

        let endpoint_url = match params {
            Some(params) => Url::parse_with_params(&base_endpoint_url, params),
            None => Url::parse(&base_endpoint_url),
        }
        .map_err(|e| ApiError::UrlParse(e.to_string()))?;

        Ok(endpoint_url)
    }
}

#[async_trait]
impl FuturesRepository for LnmFuturesRepository {
    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
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

        let url = self.get_endpoint_url(PRICE_HISTORY_PATH, Some(params))?;
        let res = reqwest::get(url).await.map_err(ApiError::Response)?;

        let price_history = res
            .json::<Vec<PriceEntryLNM>>()
            .await
            .map_err(ApiError::UnexpectedSchema)?;

        Ok(price_history)
    }
}
