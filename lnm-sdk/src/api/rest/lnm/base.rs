use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
    self, Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use std::{sync::Arc, time::Duration};

use super::super::error::{RestApiError, Result};

pub enum ApiPath {
    FuturesPriceHistory,
    FuturesTrade,
    FuturesTicker,
    FuturesCancelTrade,
    FuturesCancelAllTrades,
    FuturesCloseAllTrades,
}

impl ApiPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FuturesPriceHistory => "/v2/futures/history/price",
            Self::FuturesTrade => "/v2/futures",
            Self::FuturesTicker => "/v2/futures/ticker",
            Self::FuturesCancelTrade => "/v2/futures/cancel",
            Self::FuturesCancelAllTrades => "/v2/futures/all/cancel",
            Self::FuturesCloseAllTrades => "/v2/futures/all/close",
        }
    }
}

pub struct LnmApiBase {
    domain: String,
    key: String,
    secret: String,
    passphrase: String,
    client: Client,
}

impl LnmApiBase {
    pub fn new(
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Arc<Self>> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| RestApiError::Generic(e.to_string()))?;

        Ok(Arc::new(Self {
            domain,
            key,
            secret,
            passphrase,
            client,
        }))
    }

    fn get_url(&self, path: &ApiPath, query_params: Option<String>) -> Result<Url> {
        let query_str = query_params
            .map(|v| format!("?{v}"))
            .unwrap_or("".to_string());

        let url_str = format!("https://{}{}{}", self.domain, path.as_str(), query_str);
        let url = Url::parse(&url_str).map_err(|e| RestApiError::UrlParse(e.to_string()))?;

        Ok(url)
    }

    fn generate_signature(
        &self,
        timestamp_str: &str,
        method: &Method,
        path: &ApiPath,
        params_str: Option<impl AsRef<str>>,
    ) -> Result<String> {
        let params_str = params_str.as_ref().map(|v| v.as_ref()).unwrap_or("");

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str(),
            path.as_str(),
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
        path: &ApiPath,
        params_str: Option<String>,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut headers = HeaderMap::new();

        if authenticated {
            let timestamp = Utc::now().timestamp_millis().to_string();

            let signature =
                self.generate_signature(&timestamp, &method, path, params_str.as_ref())?;

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

                let url = self.get_url(path, None)?;
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

    pub async fn make_request_with_body<T, B>(
        &self,
        method: Method,
        path: &ApiPath,
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

    pub async fn make_request_with_query_params<I, K, V, T>(
        &self,
        method: Method,
        path: &ApiPath,
        query_params: I,
        authenticated: bool,
    ) -> Result<T>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        T: DeserializeOwned,
    {
        let query_str = query_params
            .into_iter()
            .map(|(k, v)| format!("{}={}", k.as_ref(), v.as_ref()))
            .collect::<Vec<String>>()
            .join("&");

        self.make_request(method, path, Some(query_str), authenticated)
            .await
    }

    pub async fn make_request_without_params<T>(
        &self,
        method: Method,
        path: &ApiPath,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.make_request(method, path, None, authenticated).await
    }
}
