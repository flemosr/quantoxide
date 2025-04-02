use tokio::sync::OnceCell;

pub mod error;
pub mod rest;
pub mod websocket;

use error::Result;
use rest::RestApiContext;
use websocket::WebSocketApiContext;

pub struct ApiContext {
    api_domain: String,
    rest: RestApiContext,
    ws: OnceCell<WebSocketApiContext>,
}

impl ApiContext {
    pub fn new(api_domain: String) -> Self {
        let rest = RestApiContext::new(api_domain.clone());

        Self {
            api_domain,
            rest,
            ws: OnceCell::new(),
        }
    }

    pub fn rest(&self) -> &RestApiContext {
        &self.rest
    }

    pub async fn connect_ws(&self) -> Result<&WebSocketApiContext> {
        let api_domain = self.api_domain.clone();
        let ws = self
            .ws
            .get_or_try_init(|| async { websocket::new(api_domain).await })
            .await?;

        Ok(&ws)
    }
}
