use std::{sync::Arc, time::Duration};

use tokio::sync::Mutex;

pub(crate) mod rest;
pub(crate) mod websocket;

use rest::RestClient;
use websocket::{WebSocketClient, error::Result};

#[derive(Clone, Debug)]
pub struct ApiContextConfig {
    rest_timeout: Duration,
    ws_disconnect_timeout: Duration,
}

impl Default for ApiContextConfig {
    fn default() -> Self {
        Self {
            rest_timeout: Duration::from_secs(20),
            ws_disconnect_timeout: Duration::from_secs(6),
        }
    }
}

impl ApiContextConfig {
    pub fn rest_timeout(&self) -> Duration {
        self.rest_timeout
    }

    pub fn ws_disconnect_timeout(&self) -> Duration {
        self.ws_disconnect_timeout
    }

    pub fn with_rest_timeout(mut self, timeout: Duration) -> Self {
        self.rest_timeout = timeout;
        self
    }

    pub fn with_ws_disconnect_timeout(mut self, timeout: Duration) -> Self {
        self.ws_disconnect_timeout = timeout;
        self
    }
}

pub struct ApiContext {
    config: ApiContextConfig,
    domain: String,
    pub rest: RestClient,
    ws: Mutex<Option<WebSocketClient>>,
}

impl ApiContext {
    fn new_inner(config: ApiContextConfig, domain: String, rest: RestClient) -> Arc<Self> {
        Arc::new(Self {
            config,
            domain,
            rest,
            ws: Mutex::new(None),
        })
    }

    pub fn new(config: ApiContextConfig, domain: String) -> rest::error::Result<Arc<Self>> {
        let rest = RestClient::new(&config, domain.clone())?;

        Ok(Self::new_inner(config, domain, rest))
    }

    pub fn with_credentials(
        config: ApiContextConfig,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> rest::error::Result<Arc<Self>> {
        let rest = RestClient::with_credentials(&config, domain.clone(), key, secret, passphrase)?;

        Ok(Self::new_inner(config, domain, rest))
    }

    pub async fn connect_ws(&self) -> Result<WebSocketClient> {
        let mut ws_guard = self.ws.lock().await;

        if let Some(ws) = ws_guard.as_ref() {
            if ws.is_connected().await {
                return Ok(ws.clone());
            }
        }

        let domain = self.domain.clone();
        let new_ws = websocket::new(&self.config, domain).await?;

        *ws_guard = Some(new_ws.clone());

        Ok(new_ws)
    }
}
