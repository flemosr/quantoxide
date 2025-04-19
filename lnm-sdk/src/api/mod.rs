use std::sync::Arc;
use tokio::sync::Mutex;

pub mod rest;
pub mod websocket;

use rest::RestApiContext;
use websocket::{WebSocketApiContext, error::Result};

pub struct ApiContext {
    domain: String,
    rest: RestApiContext,
    ws: Mutex<Option<Arc<WebSocketApiContext>>>,
}

impl ApiContext {
    pub fn new(
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> rest::error::Result<Arc<Self>> {
        let rest = RestApiContext::new(domain.clone(), key, secret, passphrase)?;

        Ok(Arc::new(Self {
            domain,
            rest,
            ws: Mutex::new(None),
        }))
    }

    pub fn rest(&self) -> &RestApiContext {
        &self.rest
    }

    pub async fn connect_ws(&self) -> Result<Arc<WebSocketApiContext>> {
        let mut ws_guard = self.ws.lock().await;

        if let Some(ws) = ws_guard.as_ref() {
            if ws.is_connected() {
                return Ok(ws.clone());
            }
        }

        let domain = self.domain.clone();
        let new_ws = Arc::new(websocket::new(domain).await?);

        *ws_guard = Some(new_ws.clone());

        Ok(new_ws)
    }
}
