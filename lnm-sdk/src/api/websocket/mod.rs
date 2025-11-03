use std::sync::Arc;

pub(crate) mod error;
mod lnm;
pub(crate) mod models;
mod repositories;
pub mod state; // TODO

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;
use tokio::time;

use super::ApiContextConfig;

#[derive(Clone, Debug)]
pub struct WebSocketApiConfig {
    disconnect_timeout: time::Duration,
}

impl From<&ApiContextConfig> for WebSocketApiConfig {
    fn from(value: &ApiContextConfig) -> Self {
        Self {
            disconnect_timeout: value.ws_disconnect_timeout,
        }
    }
}

impl Default for WebSocketApiConfig {
    fn default() -> Self {
        (&ApiContextConfig::default()).into()
    }
}

impl WebSocketApiConfig {
    pub fn disconnect_timeout(&self) -> time::Duration {
        self.disconnect_timeout
    }

    // pub fn set_disconnect_timeout(mut self, secs: u64) -> Self {
    //     self.disconnect_timeout = time::Duration::from_secs(secs);
    //     self
    // }
}

pub type WebSocketApiContext = Arc<dyn WebSocketRepository>;

pub async fn new(
    config: impl Into<WebSocketApiConfig>,
    api_domain: String,
) -> Result<WebSocketApiContext> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(config.into(), api_domain).await?;

    Ok(lnm_websocket_repo)
}
