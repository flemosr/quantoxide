use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
    self, Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use uuid::Uuid;

use super::super::{
    RestApiContextConfig,
    error::{RestApiError, Result},
};

#[derive(Clone)]
pub(crate) enum ApiPath {
    FuturesPriceHistory,
    FuturesTrade,
    FuturesGetTrade(Uuid),
    FuturesTicker,
    FuturesCancelTrade,
    FuturesCancelAllTrades,
    FuturesCloseAllTrades,
    FuturesAddMargin,
    FuturesCashIn,
    UserGetUser,
}

impl From<ApiPath> for String {
    fn from(value: ApiPath) -> Self {
        match value {
            ApiPath::FuturesPriceHistory => "/v2/futures/history/price".into(),
            ApiPath::FuturesTrade => "/v2/futures".into(),
            ApiPath::FuturesGetTrade(id) => format!("/v2/futures/trades/{id}"),
            ApiPath::FuturesTicker => "/v2/futures/ticker".into(),
            ApiPath::FuturesCancelTrade => "/v2/futures/cancel".into(),
            ApiPath::FuturesCancelAllTrades => "/v2/futures/all/cancel".into(),
            ApiPath::FuturesCloseAllTrades => "/v2/futures/all/close".into(),
            ApiPath::FuturesAddMargin => "/v2/futures/add-margin".into(),
            ApiPath::FuturesCashIn => "/v2/futures/cash-in".into(),
            ApiPath::UserGetUser => "/v2/user".into(),
        }
    }
}

struct LnmApiCredentials {
    key: String,
    secret: String,
    passphrase: String,
}

impl LnmApiCredentials {
    fn new(key: String, secret: String, passphrase: String) -> Self {
        Self {
            key,
            secret,
            passphrase,
        }
    }

    fn generate_signature(
        &self,
        timestamp_str: &str,
        method: &Method,
        path: ApiPath,
        params_str: Option<&String>,
    ) -> Result<String> {
        let params_str = params_str.map(|v| v.as_ref()).unwrap_or("");

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str(),
            String::from(path),
            params_str
        );

        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(RestApiError::InvalidSecretHmac)?;
        mac.update(prehash.as_bytes());
        let mac = mac.finalize().into_bytes();

        let signature = BASE64.encode(mac);

        Ok(signature)
    }

    fn get_authentication_headers(
        &self,
        method: &Method,
        path: ApiPath,
        params_str: Option<&String>,
    ) -> Result<HeaderMap> {
        let timestamp = Utc::now().timestamp_millis().to_string();

        let signature = self.generate_signature(&timestamp, &method, path, params_str)?;

        let mut headers = HeaderMap::new();

        headers.insert(
            HeaderName::from_static("lnm-access-key"),
            HeaderValue::from_str(&self.key)?,
        );
        headers.insert(
            HeaderName::from_static("lnm-access-signature"),
            HeaderValue::from_str(&signature)?,
        );
        headers.insert(
            HeaderName::from_static("lnm-access-passphrase"),
            HeaderValue::from_str(&self.passphrase)?,
        );
        headers.insert(
            HeaderName::from_static("lnm-access-timestamp"),
            HeaderValue::from_str(&timestamp)?,
        );

        Ok(headers)
    }
}

pub(crate) struct LnmRestBase {
    domain: String,
    credentials: Option<LnmApiCredentials>,
    client: Client,
}

impl LnmRestBase {
    fn new_inner(
        config: RestApiContextConfig,
        domain: String,
        credentials: Option<LnmApiCredentials>,
    ) -> Result<Arc<Self>> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(RestApiError::HttpClient)?;

        Ok(Arc::new(Self {
            domain,
            credentials,
            client,
        }))
    }

    pub fn new(config: RestApiContextConfig, domain: String) -> Result<Arc<Self>> {
        Self::new_inner(config, domain, None)
    }

    pub fn with_credentials(
        config: RestApiContextConfig,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Arc<Self>> {
        let creds = LnmApiCredentials::new(key, secret, passphrase);

        Self::new_inner(config, domain, Some(creds))
    }

    pub fn has_credentials(&self) -> bool {
        self.credentials.is_some()
    }

    fn get_url(&self, path: ApiPath, query_params: Option<String>) -> Result<Url> {
        let query_str = query_params
            .map(|v| format!("?{v}"))
            .unwrap_or("".to_string());

        let url_str = format!("https://{}{}{}", self.domain, String::from(path), query_str);
        let url = Url::parse(&url_str).map_err(|e| RestApiError::UrlParse(e.to_string()))?;

        Ok(url)
    }

    async fn make_request<T>(
        &self,
        method: Method,
        path: ApiPath,
        params_str: Option<String>,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut headers = if authenticated {
            let creds = self
                .credentials
                .as_ref()
                .ok_or(RestApiError::MissingRequestCredentials)?;

            creds.get_authentication_headers(&method, path.clone(), params_str.as_ref())?
        } else {
            HeaderMap::new()
        };

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
            m => return Err(RestApiError::UnsupportedMethod(m)),
        };

        let response = req.send().await.map_err(RestApiError::SendFailed)?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(RestApiError::ResponseDecoding)?;

            return Err(RestApiError::ErrorResponse { status, text });
        }

        let raw_response = response
            .text()
            .await
            .map_err(RestApiError::ResponseDecoding)?;

        let response_data = serde_json::from_str::<T>(&raw_response)
            .map_err(|e| RestApiError::ResponseJsonDeserializeFailed { raw_response, e })?;

        Ok(response_data)
    }

    pub async fn make_request_with_body<T, B>(
        &self,
        method: Method,
        path: ApiPath,
        body: B,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let body =
            serde_json::to_string(&body).map_err(RestApiError::RequestJsonSerializeFailed)?;

        self.make_request(method, path, Some(body), authenticated)
            .await
    }

    pub async fn make_request_with_query_params<I, K, V, T>(
        &self,
        method: Method,
        path: ApiPath,
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
        path: ApiPath,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.make_request(method, path, None, authenticated).await
    }
}
