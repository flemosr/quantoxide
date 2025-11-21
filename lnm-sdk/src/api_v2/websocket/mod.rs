use std::sync::Arc;

use crate::shared::config::WebSocketClientConfig;

pub(in crate::api_v2) mod error;
mod lnm;
pub(in crate::api_v2) mod models;
pub(in crate::api_v2) mod repositories;
pub(in crate::api_v2) mod state;

use error::Result;
use lnm::LnmWebSocketRepo;
use repositories::WebSocketRepository;
use tokio::sync::Mutex;

/// Thread-safe handle to a [`WebSocketRepository`].
pub type WebSocketConnection = Arc<dyn WebSocketRepository>;

/// Client for interacting with the [LNM's v2 API] via WebSocket.
///
/// `WebSocketClient` provides a thread-safe interface to establish and reuse WebSocket
/// connections. It automatically reuses existing active connections and creates new ones
/// when needed.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
pub struct WebSocketClient {
    config: WebSocketClientConfig,
    domain: String,
    conn: Mutex<Option<WebSocketConnection>>,
}

impl WebSocketClient {
    /// Creates a new WebSocket client.
    ///
    /// The client is created in a disconnected state. Use [`WebSocketClient::connect`] to
    /// establish a connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() {
    /// use lnm_sdk::api_v2::{WebSocketClient, WebSocketClientConfig};
    ///
    /// let config = WebSocketClientConfig::default();
    /// let client = WebSocketClient::new(config, "api.lnmarkets.com");
    /// # }
    /// ```
    pub fn new(config: impl Into<WebSocketClientConfig>, domain: impl ToString) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            domain: domain.to_string(),
            conn: Mutex::new(None),
        })
    }

    /// Connects to the WebSocket API or returns an existing connection.
    ///
    /// This method handles connection establishment and reuse automatically:
    /// + If a connection already exists and is active, it returns that connection
    /// + If no connection exists or the existing one is disconnected, it creates a new one
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v2::{WebSocketClient, WebSocketClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = WebSocketClient::new(WebSocketClientConfig::default(), domain);
    ///
    /// let ws = client.connect().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(&self) -> Result<WebSocketConnection> {
        let mut conn_guard = self.conn.lock().await;

        if let Some(conn) = conn_guard.as_ref() {
            if conn.is_connected().await {
                return Ok(conn.clone());
            }
        }

        let new_conn =
            Arc::new(LnmWebSocketRepo::new(self.config.clone(), self.domain.clone()).await?);

        *conn_guard = Some(new_conn.clone());

        Ok(new_conn)
    }
}
