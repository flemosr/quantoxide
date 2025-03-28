use reqwest::Url;
use std::borrow::Borrow;
use tokio::sync::OnceCell;

pub mod error;
pub mod rest;
pub mod websocket;

use error::{ApiError, Result};

static API_DOMAIN: OnceCell<String> = OnceCell::const_new();

pub fn init(api_base_url: String) -> Result<()> {
    API_DOMAIN
        .set(api_base_url)
        .map_err(|_| ApiError::Init("`api` must not be initialized"))?;

    Ok(())
}

fn get_api_domain() -> Result<&'static String> {
    let api_domain = API_DOMAIN
        .get()
        .ok_or_else(|| ApiError::Init("`api` must be initialized"))?;
    Ok(api_domain)
}

fn get_endpoint_url<I, K, V>(path: impl AsRef<str>, params: Option<I>) -> Result<Url>
where
    I: IntoIterator,
    I::Item: Borrow<(K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let api_domain = get_api_domain()?;
    let base_endpoint_url = format!("https://{api_domain}{}", path.as_ref());

    let endpoint_url = match params {
        Some(params) => Url::parse_with_params(&base_endpoint_url, params),
        None => Url::parse(&base_endpoint_url),
    }
    .map_err(|e| ApiError::UrlParse(e.to_string()))?;

    Ok(endpoint_url)
}
