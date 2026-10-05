use std::borrow::Cow;

use serde_json::Value;
use thiserror::Error;

pub const API_KEY_HEADER: &str = "X-API-Key";

#[derive(Debug, Error)]
pub enum FxMacroDataError {
    #[error("missing required FXMacroData field `{0}`")]
    MissingField(&'static str),

    #[error("unsupported FXMacroData endpoint `{0}`")]
    UnsupportedEndpoint(String),

    #[error("FXMacroData returned an unexpected redirect (HTTP {0})")]
    UnexpectedRedirect(u16),

    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FxMacroDataEndpoint {
    DataCatalogue,
    Announcements,
    LatestAnnouncements,
    AnnouncementChanges,
    Calendar,
    Predictions,
    Forex,
    Cot,
    Commodity,
    CommoditiesLatest,
    Curves,
    CurveProxies,
    ForwardCurves,
    RateDifferentials,
    ForwardDifferentials,
    MarketSessions,
    RiskSentiment,
    News,
    PressReleases,
    Graphql,
    Custom,
}

#[derive(Clone, Debug)]
pub struct FxMacroDataRequest {
    pub endpoint: FxMacroDataEndpoint,
    pub currency: Option<String>,
    pub indicator: Option<String>,
    pub base: Option<String>,
    pub quote: Option<String>,
    pub path: Option<String>,
    pub params: Vec<(String, String)>,
    pub body: Option<Value>,
}

impl FxMacroDataRequest {
    #[must_use]
    pub fn new(endpoint: FxMacroDataEndpoint) -> Self {
        Self {
            endpoint,
            currency: None,
            indicator: None,
            base: None,
            quote: None,
            path: None,
            params: Vec::new(),
            body: None,
        }
    }

    /// Select one page of a list endpoint. List endpoints return 20 rows by
    /// default and at most 100 per request, newest first; request the next
    /// page with the response's `pagination.next_offset` while
    /// `pagination.has_more` is true.
    #[must_use]
    pub fn page(mut self, limit: u32, offset: u32) -> Self {
        self.params
            .retain(|(key, _)| key != "limit" && key != "offset");
        self.params
            .push(("limit".to_owned(), limit.clamp(1, 100).to_string()));
        self.params.push(("offset".to_owned(), offset.to_string()));
        self
    }
}

#[derive(Clone)]
pub struct FxMacroDataClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl Default for FxMacroDataClient {
    fn default() -> Self {
        Self::new(
            std::env::var("FXMACRODATA_API_KEY")
                .or_else(|_| std::env::var("FXMD_API_KEY"))
                .ok(),
        )
    }
}

impl FxMacroDataClient {
    #[must_use]
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "https://api.fxmacrodata.com/v1")
    }

    #[must_use]
    pub fn with_base_url(api_key: Option<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.filter(|key| !key.trim().is_empty()),
            // Redirects are not followed so the API key header is never replayed
            // to another host.
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("FXMacroData HTTP client"),
        }
    }

    /// Header sent with each request, or `None` when no key is set.
    #[must_use]
    pub fn api_key_header(&self) -> Option<(&'static str, &str)> {
        self.api_key.as_deref().map(|key| (API_KEY_HEADER, key))
    }

    pub async fn request_json(
        &self,
        request: FxMacroDataRequest,
    ) -> Result<Value, FxMacroDataError> {
        let method = self.method(&request);
        let url = self.build_url(&request)?;
        let mut http = if method == "POST" {
            self.http
                .post(url)
                .json(&request.body.unwrap_or(Value::Null))
        } else {
            self.http.get(url)
        };
        if let Some((name, key)) = self.api_key_header() {
            http = http.header(name, key);
        }
        let response = http.send().await?;
        if response.status().is_redirection() {
            return Err(FxMacroDataError::UnexpectedRedirect(response.status().as_u16()));
        }
        Ok(response.error_for_status()?.json().await?)
    }

    pub async fn data_catalogue(&self, currency: &str) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::DataCatalogue);
        request.currency = Some(currency.to_owned());
        self.request_json(request).await
    }

    /// Returns the first page (20 rows, newest first). For more rows or older
    /// pages, send the request through [`Self::request_json`] with
    /// [`FxMacroDataRequest::page`].
    pub async fn announcements(
        &self,
        currency: &str,
        indicator: &str,
    ) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Announcements);
        request.currency = Some(currency.to_owned());
        request.indicator = Some(indicator.to_owned());
        self.request_json(request).await
    }

    pub async fn latest_announcements(&self, currency: &str) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::LatestAnnouncements);
        request.currency = Some(currency.to_owned());
        self.request_json(request).await
    }

    pub async fn calendar(&self, currency: &str) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Calendar);
        request.currency = Some(currency.to_owned());
        self.request_json(request).await
    }

    /// Returns the first page (20 rows, newest first). For more rows or older
    /// pages, send the request through [`Self::request_json`] with
    /// [`FxMacroDataRequest::page`].
    pub async fn predictions(
        &self,
        currency: &str,
        indicator: &str,
    ) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Predictions);
        request.currency = Some(currency.to_owned());
        request.indicator = Some(indicator.to_owned());
        self.request_json(request).await
    }

    /// Returns the first page (20 rows, newest first). For more rows or older
    /// pages, send the request through [`Self::request_json`] with
    /// [`FxMacroDataRequest::page`].
    pub async fn forex(&self, base: &str, quote: &str) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Forex);
        request.base = Some(base.to_owned());
        request.quote = Some(quote.to_owned());
        self.request_json(request).await
    }

    pub async fn graphql(&self, query: &str, variables: Value) -> Result<Value, FxMacroDataError> {
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Graphql);
        request.body = Some(serde_json::json!({ "query": query, "variables": variables }));
        self.request_json(request).await
    }

    pub fn build_url(&self, request: &FxMacroDataRequest) -> Result<String, FxMacroDataError> {
        let params = &request.params;
        let mut url = format!("{}{}", self.base_url, self.path(request)?);
        if !params.is_empty() {
            url.push('?');
            url.push_str(
                &params
                    .iter()
                    .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        Ok(url)
    }

    fn method(&self, request: &FxMacroDataRequest) -> &'static str {
        if matches!(request.endpoint, FxMacroDataEndpoint::Graphql)
            || (matches!(request.endpoint, FxMacroDataEndpoint::Custom) && request.body.is_some())
        {
            "POST"
        } else {
            "GET"
        }
    }

    fn path(&self, request: &FxMacroDataRequest) -> Result<String, FxMacroDataError> {
        let path = match request.endpoint {
            FxMacroDataEndpoint::DataCatalogue => {
                format!(
                    "/data_catalogue/{}",
                    segment(&request.currency, "currency")?
                )
            }
            FxMacroDataEndpoint::Announcements => format!(
                "/announcements/{}/{}",
                segment(&request.currency, "currency")?,
                segment(&request.indicator, "indicator")?
            ),
            FxMacroDataEndpoint::LatestAnnouncements => {
                format!(
                    "/announcements/{}/latest",
                    segment(&request.currency, "currency")?
                )
            }
            FxMacroDataEndpoint::AnnouncementChanges => "/announcements/changes".to_owned(),
            FxMacroDataEndpoint::Calendar => {
                format!("/calendar/{}", segment(&request.currency, "currency")?)
            }
            FxMacroDataEndpoint::Predictions => format!(
                "/predictions/{}/{}",
                segment(&request.currency, "currency")?,
                segment(&request.indicator, "indicator")?
            ),
            FxMacroDataEndpoint::Forex => format!(
                "/forex/{}/{}",
                segment(&request.base, "base")?,
                segment(&request.quote, "quote")?
            ),
            FxMacroDataEndpoint::Cot => format!("/cot/{}", segment(&request.currency, "currency")?),
            FxMacroDataEndpoint::Commodity => {
                format!("/commodities/{}", segment(&request.indicator, "indicator")?)
            }
            FxMacroDataEndpoint::CommoditiesLatest => "/commodities/latest".to_owned(),
            FxMacroDataEndpoint::Curves => {
                format!("/curves/{}", segment(&request.currency, "currency")?)
            }
            FxMacroDataEndpoint::CurveProxies => {
                format!("/curve_proxies/{}", segment(&request.currency, "currency")?)
            }
            FxMacroDataEndpoint::ForwardCurves => {
                format!(
                    "/forward_curves/{}",
                    segment(&request.currency, "currency")?
                )
            }
            FxMacroDataEndpoint::RateDifferentials => format!(
                "/rate_differentials/{}/{}",
                segment(&request.base, "base")?,
                segment(&request.quote, "quote")?
            ),
            FxMacroDataEndpoint::ForwardDifferentials => format!(
                "/forward_differentials/{}/{}",
                segment(&request.base, "base")?,
                segment(&request.quote, "quote")?
            ),
            FxMacroDataEndpoint::MarketSessions => "/market_sessions".to_owned(),
            FxMacroDataEndpoint::RiskSentiment => "/risk_sentiment".to_owned(),
            FxMacroDataEndpoint::News => {
                format!("/news/{}", segment(&request.currency, "currency")?)
            }
            FxMacroDataEndpoint::PressReleases => {
                format!(
                    "/press-releases/{}",
                    segment(&request.currency, "currency")?
                )
            }
            FxMacroDataEndpoint::Graphql => "/graphql".to_owned(),
            FxMacroDataEndpoint::Custom => request
                .path
                .as_deref()
                .map(|path| {
                    if path.starts_with('/') {
                        path.to_owned()
                    } else {
                        format!("/{path}")
                    }
                })
                .ok_or(FxMacroDataError::MissingField("path"))?,
        };
        Ok(path)
    }
}

impl std::fmt::Debug for FxMacroDataClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxMacroDataClient")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

fn segment(value: &Option<String>, name: &'static str) -> Result<String, FxMacroDataError> {
    value
        .as_deref()
        .map(str::to_lowercase)
        .map(|value| encode(&value).into_owned())
        .ok_or(FxMacroDataError::MissingField(name))
}

fn encode(value: &str) -> Cow<'_, str> {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(
            value
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                        char::from(b).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{FxMacroDataClient, FxMacroDataEndpoint, FxMacroDataRequest};

    #[test]
    fn builds_macro_urls_without_key() {
        let client = FxMacroDataClient::with_base_url(
            Some("test-key".to_owned()),
            "https://api.fxmacrodata.com/v1/",
        );
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Predictions);
        request.currency = Some("USD".to_owned());
        request.indicator = Some("non_farm_payrolls".to_owned());
        request.params.push(("limit".to_owned(), "1".to_owned()));

        assert_eq!(
            client.build_url(&request).unwrap(),
            "https://api.fxmacrodata.com/v1/predictions/usd/non_farm_payrolls?limit=1"
        );
        assert_eq!(client.api_key_header(), Some(("X-API-Key", "test-key")));
    }

    #[test]
    fn page_sets_limit_and_offset() {
        let client = FxMacroDataClient::new(None);
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::Forex);
        request.base = Some("EUR".to_owned());
        request.quote = Some("USD".to_owned());
        let request = request.page(500, 200);

        assert_eq!(
            client.build_url(&request).unwrap(),
            "https://api.fxmacrodata.com/v1/forex/eur/usd?limit=100&offset=200"
        );
    }

    #[test]
    fn omits_header_without_key() {
        assert_eq!(FxMacroDataClient::new(None).api_key_header(), None);
        assert_eq!(
            FxMacroDataClient::new(Some(String::new())).api_key_header(),
            None
        );
    }

    #[test]
    fn builds_cross_currency_market_urls() {
        let client = FxMacroDataClient::new(Some("test-key".to_owned()));
        let mut request = FxMacroDataRequest::new(FxMacroDataEndpoint::RateDifferentials);
        request.base = Some("EUR".to_owned());
        request.quote = Some("USD".to_owned());
        request.params.push(("tenor".to_owned(), "2y".to_owned()));

        assert_eq!(
            client.build_url(&request).unwrap(),
            "https://api.fxmacrodata.com/v1/rate_differentials/eur/usd?tenor=2y"
        );
    }
}
