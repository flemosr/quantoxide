use std::borrow::Borrow;
use std::time::Duration;

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{
    self, Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;

use super::super::{
    error::{RestApiError, Result},
    models::{FuturesTradeRequestBody, PriceEntryLNM, Trade, TradeSide, TradeType},
    repositories::FuturesRepository,
};

const PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";
const CREATE_NEW_TRADE_PATH: &str = "/v2/futures";

pub struct LnmFuturesRepository {
    domain: String,
    key: String,
    secret: String,
    passphrase: String,
    client: Client,
}

impl LnmFuturesRepository {
    pub fn new(domain: String, key: String, secret: String, passphrase: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| RestApiError::Generic(e.to_string()))?;

        Ok(Self {
            domain,
            key,
            secret,
            passphrase,
            client,
        })
    }

    fn get_endpoint_url<I, K, V>(&self, path: impl AsRef<str>, params: Option<I>) -> Result<Url>
    where
        I: IntoIterator,
        I::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let base_endpoint_url = format!("https://{}{}", self.domain, path.as_ref());

        let endpoint_url = match params {
            Some(params) => Url::parse_with_params(&base_endpoint_url, params),
            None => Url::parse(&base_endpoint_url),
        }
        .map_err(|e| RestApiError::UrlParse(e.to_string()))?;

        Ok(endpoint_url)
    }

    fn get_url(&self, path: impl AsRef<str>, query_params: Option<String>) -> Result<Url> {
        let query_str = query_params
            .map(|v| format!("?{v}"))
            .unwrap_or("".to_string());

        let url_str = format!("https://{}{}{}", self.domain, path.as_ref(), query_str);
        let url = Url::parse(&url_str).map_err(|e| RestApiError::UrlParse(e.to_string()))?;

        Ok(url)
    }

    fn generate_signature(
        &self,
        timestamp_str: &str,
        method: &Method,
        path: impl AsRef<str>,
        params_str: Option<impl AsRef<str>>,
    ) -> Result<String> {
        let params_str = params_str.as_ref().map(|v| v.as_ref()).unwrap_or("");

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str(),
            path.as_ref(),
            params_str
        );

        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(|_| RestApiError::Generic("HMAC error".to_string()))?;
        mac.update(prehash.as_bytes());
        let mac = mac.finalize().into_bytes();

        let signature = BASE64.encode(mac);

        Ok(signature)
    }

    async fn make_request<T>(
        &self,
        method: Method,
        path: impl AsRef<str>,
        params_str: Option<String>,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let mut headers = HeaderMap::new();

        if authenticated {
            let timestamp = Utc::now().timestamp_millis().to_string();

            let signature =
                self.generate_signature(&timestamp, &method, &path, params_str.clone())?;

            headers.insert(
                HeaderName::from_static("lnm-access-key"),
                HeaderValue::from_str(&self.key)
                    .map_err(|e| RestApiError::Generic(e.to_string()))?,
            );
            headers.insert(
                HeaderName::from_static("lnm-access-signature"),
                HeaderValue::from_str(&signature)
                    .map_err(|e| RestApiError::Generic(e.to_string()))?,
            );
            headers.insert(
                HeaderName::from_static("lnm-access-passphrase"),
                HeaderValue::from_str(&self.passphrase)
                    .map_err(|e| RestApiError::Generic(e.to_string()))?,
            );
            headers.insert(
                HeaderName::from_static("lnm-access-timestamp"),
                HeaderValue::from_str(&timestamp)
                    .map_err(|e| RestApiError::Generic(e.to_string()))?,
            );
        }

        let req = match method {
            Method::POST | Method::PUT => {
                headers.insert(
                    HeaderName::from_static("content-type"),
                    HeaderValue::from_static("application/json"),
                );

                let url = self.get_url(path.as_ref(), None)?;
                let mut req = self.client.request(method, url).headers(headers);
                if let Some(body) = params_str {
                    req = req.body(body);
                }
                req
            }
            Method::GET | Method::DELETE => {
                let url = self.get_url(path, params_str)?;
                self.client.request(method, url).headers(headers)
            }
            _ => return Err(RestApiError::Generic("invalid method".to_string())),
        };

        let response = req
            .send()
            .await
            .map_err(|e| RestApiError::Generic(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .map_err(|e| RestApiError::Generic(format!("{:?}, {}", e, status)))?;

            return Err(RestApiError::Generic(error_text));
        }

        let response_data = response
            .json::<T>()
            .await
            .map_err(|e| RestApiError::Generic(e.to_string()))?;

        Ok(response_data)
    }

    async fn make_request_with_body<T, B>(
        &self,
        method: Method,
        path: impl AsRef<str>,
        body: B,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let body =
            serde_json::to_string(&body).map_err(|e| RestApiError::Generic(e.to_string()))?;

        self.make_request(method, path, Some(body), authenticated)
            .await
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
        let res = reqwest::get(url).await.map_err(RestApiError::Response)?;

        let price_history = res
            .json::<Vec<PriceEntryLNM>>()
            .await
            .map_err(RestApiError::UnexpectedSchema)?;

        Ok(price_history)
    }

    async fn create_new_trade_margin_limit(
        &self,
        side: TradeSide,
        margin: u64,
        leverage: f64,
        price: f64,
        stoploss: Option<f64>,
        takeprofit: Option<f64>,
    ) -> Result<Trade> {
        let body = FuturesTradeRequestBody {
            side,
            trade_type: TradeType::L,
            margin,
            leverage,
            price,
            stoploss,
            takeprofit,
        };

        let created_trade: Trade = self
            .make_request_with_body(Method::POST, CREATE_NEW_TRADE_PATH, Some(body), true)
            .await?;

        Ok(created_trade)
    }
}
