use std::sync::Arc;
use tokio::sync::Mutex;

pub mod error;
pub mod rest;
pub mod websocket;

use error::Result;
use rest::RestApiContext;
use websocket::WebSocketApiContext;

pub struct ApiContext {
    api_domain: String,
    rest: RestApiContext,
    ws: Mutex<Option<Arc<WebSocketApiContext>>>,
}

impl ApiContext {
    pub fn new(api_domain: String) -> Arc<Self> {
        let rest = RestApiContext::new(api_domain.clone());

        Arc::new(Self {
            api_domain,
            rest,
            ws: Mutex::new(None),
        })
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

        let api_domain = self.api_domain.clone();
        let new_ws = Arc::new(websocket::new(api_domain).await?);

        *ws_guard = Some(new_ws.clone());

        Ok(new_ws)
    }
}
