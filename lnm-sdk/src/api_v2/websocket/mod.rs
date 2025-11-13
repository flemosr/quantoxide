use std::sync::Arc;

pub(in crate::api_v2) mod error;
mod lnm;
pub(in crate::api_v2) mod models;
pub(in crate::api_v2) mod repositories;
pub(in crate::api_v2) mod state;

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;
use tokio::time;

use super::client::ApiClientConfig;

#[derive(Clone, Debug)]
pub(in crate::api_v2) struct WebSocketClientConfig {
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

pub(in crate::api_v2) async fn new(
    config: impl Into<WebSocketClientConfig>,
    api_domain: String,
) -> Result<WebSocketClient> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(config.into(), api_domain).await?;

    Ok(lnm_websocket_repo)
}
