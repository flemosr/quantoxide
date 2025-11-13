use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::{
    self, Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};

use super::super::{
    super::config::RestClientConfig,
    error::{RestApiError, Result},
};

pub(crate) trait SignatureGenerator: Send + Sync {
    fn generate<P: RestPath>(
        &self,
        timestamp: DateTime<Utc>,
        method: &Method,
        path: P,
        params_str: Option<&String>,
    ) -> Result<String>;
}

pub(crate) trait RestPath: Clone {
    fn to_path_string(self) -> String;
}

struct LnmRestCredentials<S: SignatureGenerator> {
    key: String,
    passphrase: String,
    signature_generator: S,
}

impl<S: SignatureGenerator> LnmRestCredentials<S> {
    fn new(key: String, passphrase: String, signature_generator: S) -> Self {
        Self {
            key,
            passphrase,
            signature_generator,
        }
    }

    fn generate_signature(
        &self,
        timestamp: DateTime<Utc>,
        method: &Method,
        path: impl RestPath,
        params_str: Option<&String>,
    ) -> Result<String> {
        self.signature_generator
            .generate(timestamp, method, path, params_str)
    }

    fn get_authentication_headers(
        &self,
        method: &Method,
        path: impl RestPath,
        params_str: Option<&String>,
    ) -> Result<HeaderMap> {
        let timestamp = Utc::now();

        let signature = self.generate_signature(timestamp, &method, path, params_str)?;

        let timestamp_str = timestamp.timestamp_millis().to_string();

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
            HeaderValue::from_str(&timestamp_str)?,
        );

        Ok(headers)
    }
}

pub(crate) struct LnmRestBase<S: SignatureGenerator> {
    domain: String,
    credentials: Option<LnmRestCredentials<S>>,
    client: Client,
}

impl<S: SignatureGenerator> LnmRestBase<S> {
    pub fn new(config: RestClientConfig, domain: String) -> Result<Arc<Self>> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .map_err(RestApiError::HttpClient)?;

        Ok(Arc::new(Self {
            domain,
            credentials: None,
            client,
        }))
    }

    pub fn with_credentials(
        config: RestClientConfig,
        domain: String,
        key: String,
        passphrase: String,
        signature_generator: S,
    ) -> Result<Arc<Self>> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .map_err(RestApiError::HttpClient)?;

        let creds = LnmRestCredentials::new(key, passphrase, signature_generator);

        Ok(Arc::new(Self {
            domain,
            credentials: Some(creds),
            client,
        }))
    }

    pub fn has_credentials(&self) -> bool {
        self.credentials.is_some()
    }

    fn get_url(&self, path: impl RestPath, query_params: Option<String>) -> Result<Url> {
        let query_str = query_params
            .map(|v| format!("?{v}"))
            .unwrap_or("".to_string());

        let url_str = format!(
            "https://{}{}{}",
            self.domain,
            path.to_path_string(),
            query_str
        );
        let url = Url::parse(&url_str).map_err(|e| RestApiError::UrlParse(e.to_string()))?;

        Ok(url)
    }

    async fn make_request<T>(
        &self,
        method: Method,
        path: impl RestPath,
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
        path: impl RestPath,
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
        path: impl RestPath,
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
        path: impl RestPath,
        authenticated: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.make_request(method, path, None, authenticated).await
    }
}
