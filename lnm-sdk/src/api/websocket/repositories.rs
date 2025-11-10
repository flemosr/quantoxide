use std::collections::HashSet;

use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;

use super::{
    error::Result,
    models::{LnmWebSocketChannel, WebSocketUpdate},
    state::WsConnectionStatus,
};

/// Methods to interacting with LNM's WebSocket API.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
#[async_trait]
pub trait WebSocketRepository: crate::sealed::Sealed + Send + Sync {
    /// Returns whether the WebSocket connection is currently established.
    ///
    /// This is a convenience method that checks if the current is `Connected`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// if ws.is_connected().await {
    ///     // WebSocket is connected
    /// } else {
    ///     // WebSocket is not connected
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn is_connected(&self) -> bool;

    /// Returns the current connection status of the WebSocket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, WsConnectionStatus};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// match ws.connection_status().await {
    ///     WsConnectionStatus::Connected => {
    ///         // ...
    ///     },
    ///     WsConnectionStatus::DisconnectInitiated => {
    ///         // ...
    ///     },
    ///     WsConnectionStatus::Disconnected => {
    ///         // ...
    ///     },
    ///     WsConnectionStatus::Failed(err) => {
    ///         // ...
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn connection_status(&self) -> WsConnectionStatus;

    /// Subscribes to the specified WebSocket channels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, models::LnmWebSocketChannel};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// ws.subscribe(vec![
    ///     LnmWebSocketChannel::FuturesBtcUsdIndex,
    ///     LnmWebSocketChannel::FuturesBtcUsdLastPrice
    /// ]).await?;
    ///
    /// assert_eq!(ws.subscriptions().await.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    /// Unsubscribes from the specified WebSocket channels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, models::LnmWebSocketChannel};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// ws.subscribe(vec![LnmWebSocketChannel::FuturesBtcUsdIndex]).await?;
    /// ws.unsubscribe(vec![LnmWebSocketChannel::FuturesBtcUsdIndex]).await?;
    ///
    /// assert!(ws.subscriptions().await.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    /// Returns the set of currently subscribed channels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, models::LnmWebSocketChannel};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// ws.subscribe(vec![
    ///     LnmWebSocketChannel::FuturesBtcUsdIndex,
    ///     LnmWebSocketChannel::FuturesBtcUsdLastPrice
    /// ]).await?;
    ///
    /// assert_eq!(ws.subscriptions().await.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel>;

    /// Creates a new receiver for WebSocket updates.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, models::LnmWebSocketChannel};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// let mut ws_rx = ws.receiver().await?;
    ///
    /// ws.subscribe(vec![LnmWebSocketChannel::FuturesBtcUsdIndex]).await?;
    ///
    /// while let Ok(ws_update) = ws_rx.recv().await {
    ///     // Handle `ws_update`
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn receiver(&self) -> Result<Receiver<WebSocketUpdate>>;

    /// Disconnects the WebSocket connection. After disconnection, all receivers will stop receiving
    /// updates and the connection status will change to `Disconnected`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::{ApiClient, ApiClientConfig, WsConnectionStatus};
    ///
    /// let api_domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let client = ApiClient::new(ApiClientConfig::default(), api_domain)?;
    /// let ws = client.connect_ws().await?;
    ///
    /// assert!(matches!(ws.connection_status().await, WsConnectionStatus::Connected));
    ///
    /// ws.disconnect().await?;
    ///
    /// assert!(matches!(ws.connection_status().await, WsConnectionStatus::Disconnected));
    /// # Ok(())
    /// # }
    /// ```
    async fn disconnect(&self) -> Result<()>;
}
