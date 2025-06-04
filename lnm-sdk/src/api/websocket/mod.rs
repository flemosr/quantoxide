use std::sync::Arc;

pub mod error;
mod lnm;
pub mod models;
mod repositories;

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;
use tokio::time;

#[derive(Clone, Debug)]
pub struct WebSocketApiConfig {
    disconnect_timeout: time::Duration,
}

impl Default for WebSocketApiConfig {
    fn default() -> Self {
        Self {
            disconnect_timeout: time::Duration::from_secs(6),
        }
    }
}

impl WebSocketApiConfig {
    pub fn disconnect_timeout(&self) -> time::Duration {
        self.disconnect_timeout
    }

    pub fn set_disconnect_timeout(mut self, secs: u64) -> Self {
        self.disconnect_timeout = time::Duration::from_secs(secs);
        self
    }
}

pub type WebSocketApiContext = Arc<dyn WebSocketRepository>;

pub async fn new(config: WebSocketApiConfig, api_domain: String) -> Result<WebSocketApiContext> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(config, api_domain).await?;

    Ok(lnm_websocket_repo)
}
