use std::{ops::Deref, sync::Arc};

use crate::shared::config::WebSocketClientConfig;

pub(in crate::api_v2) mod error;
mod lnm;
pub(in crate::api_v2) mod models;
pub(in crate::api_v2) mod repositories;
pub(in crate::api_v2) mod state;

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;

/// Handle to a [`WebSocketRepository`].
pub struct WebSocketClient(Box<dyn WebSocketRepository>);

impl WebSocketClient {
    pub async fn new(
        config: impl Into<WebSocketClientConfig>,
        domain: impl ToString,
    ) -> Result<Arc<Self>> {
        let ws_repo = Box::new(LnmWebSocketRepo::new(config.into(), domain.to_string()).await?);

        Ok(Arc::new(Self(ws_repo)))
    }
}

impl Deref for WebSocketClient {
    type Target = dyn WebSocketRepository;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
