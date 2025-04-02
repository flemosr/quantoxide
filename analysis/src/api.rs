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
