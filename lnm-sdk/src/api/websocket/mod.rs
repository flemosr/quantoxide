use std::sync::Arc;

pub(crate) mod error;
mod lnm;
pub(crate) mod models;
pub(crate) mod repositories;
pub(crate) mod state;

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;
use tokio::time;

use super::client::ApiClientConfig;

#[derive(Clone, Debug)]
pub(crate) struct WebSocketClientConfig {
    disconnect_timeout: time::Duration,
}

impl From<&ApiClientConfig> for WebSocketClientConfig {
    fn from(value: &ApiClientConfig) -> Self {
        Self {
            disconnect_timeout: value.ws_disconnect_timeout(),
        }
    }
}

impl WebSocketClientConfig {
    pub fn disconnect_timeout(&self) -> time::Duration {
        self.disconnect_timeout
    }
}

/// Thread-safe handle to a [`WebSocketRepository`].
pub type WebSocketClient = Arc<dyn WebSocketRepository>;

pub(crate) async fn new(
    config: impl Into<WebSocketClientConfig>,
    api_domain: String,
) -> Result<WebSocketClient> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(config.into(), api_domain).await?;

    Ok(lnm_websocket_repo)
}
