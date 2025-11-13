use std::sync::Arc;

use tokio::sync::Mutex;

use crate::shared::{config::ApiClientConfig, rest::error::Result as RestResult};

use super::{
    rest::RestClient,
    websocket::{self, WebSocketClient, error::Result},
};

/// Client for interacting with the [LNM's v2 API] via REST and WebSocket.
///
/// `ApiClient` provides a unified interface for making REST API calls and establishing WebSocket
/// connections. It manages the WebSocket connection lifecycle and, when credentials are provided,
/// authentication.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
pub struct ApiClient {
    config: ApiClientConfig,
    domain: String,
    pub rest: RestClient,
    ws: Mutex<Option<WebSocketClient>>,
}

impl ApiClient {
    fn new_inner(config: ApiClientConfig, domain: String, rest: RestClient) -> Arc<Self> {
        Arc::new(Self {
            config,
            domain,
            rest,
            ws: Mutex::new(None),
        })
    }

    /// Creates a new unauthenticated API client.
    ///
    /// For authenticated endpoints, use [`ApiClient::with_credentials`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v2::{ApiClient, ApiClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    ///
    /// let api = ApiClient::new(ApiClientConfig::default(), domain)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: ApiClientConfig, domain: String) -> RestResult<Arc<Self>> {
        let rest = RestClient::new(&config, domain.clone())?;

        Ok(Self::new_inner(config, domain, rest))
    }

    /// Creates a new authenticated API client with credentials.
    ///
    /// If not accessing authenticated endpoints, consider using [`ApiClient::new`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v2::{ApiClient, ApiClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let key = env::var("LNM_API_KEY").unwrap();
    /// let secret = env::var("LNM_API_SECRET").unwrap();
    /// let pphrase = env::var("LNM_API_PASSPHRASE").unwrap();
    ///
    /// let config = ApiClientConfig::default();
    /// let api = ApiClient::with_credentials(config, domain, key, secret, pphrase)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_credentials(
        config: ApiClientConfig,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> RestResult<Arc<Self>> {
        let rest = RestClient::with_credentials(&config, domain.clone(), key, secret, passphrase)?;

        Ok(Self::new_inner(config, domain, rest))
    }

    /// Connects to the WebSocket API or returns an existing connection.
    ///
    /// This method manages WebSocket connection lifecycle automatically:
    /// + If a connection already exists and is active, it returns that connection
    /// + If no connection exists or the existing one is disconnected, it creates a new one
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v2::{ApiClient, ApiClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let api = ApiClient::new(ApiClientConfig::default(), domain)?;
    ///
    /// let ws = api.connect_ws().await?;
    /// # Ok(())
    /// # }
    /// ```
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
