use reqwest::Url;
use std::borrow::Borrow;
use tokio::sync::OnceCell;

use crate::Result;

pub mod models;
pub mod rest;

static API_BASE_URL: OnceCell<String> = OnceCell::const_new();

pub fn init(api_base_url: String) {
    API_BASE_URL
        .set(api_base_url)
        .expect("`api` must not be initialized");
}

fn get_endpoint_url<I, K, V>(path: impl AsRef<str>, params: Option<I>) -> Result<Url>
where
    I: IntoIterator,
    I::Item: Borrow<(K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let base_url = API_BASE_URL.get().expect("`api` must be initialized");
    let base_endpoint_url = base_url.clone() + path.as_ref();

    let endpoint_url = match params {
        Some(params) => Url::parse_with_params(&base_endpoint_url, params)?,
        None => Url::parse(&base_endpoint_url)?,
    };

    Ok(endpoint_url)
}
