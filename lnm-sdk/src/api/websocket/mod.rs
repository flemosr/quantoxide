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
    shutdown_timeout: time::Duration,
}

impl Default for WebSocketApiConfig {
    fn default() -> Self {
        Self {
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl WebSocketApiConfig {
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

pub type WebSocketApiContext = Box<dyn WebSocketRepository>;

pub async fn new(config: WebSocketApiConfig, api_domain: String) -> Result<WebSocketApiContext> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(config, api_domain).await?;
    let ws_api_context = Box::new(lnm_websocket_repo);

    Ok(ws_api_context)
}
