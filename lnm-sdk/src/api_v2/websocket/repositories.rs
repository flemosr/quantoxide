use std::collections::HashSet;

use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;

use super::{
    error::Result,
    models::{WebSocketChannel, WebSocketUpdate},
    state::WsConnectionStatus,
};

/// Methods for interacting with [LNM's v2 API]'s WebSocket.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait WebSocketRepository: crate::sealed::Sealed + Send + Sync {
    /// Returns whether the WebSocket connection is currently established.
    ///
    /// This is a convenience method that checks if the current status is `Connected`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let ws = ws_api.connect().await?;
    ///
    /// if ws.is_connected().await {
    ///     // ...
    /// } else {
    ///     // ...
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
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WsConnectionStatus;
    /// let ws = ws_api.connect().await?;
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
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WebSocketChannel;
    /// let ws = ws_api.connect().await?;
    ///
    /// ws.subscribe(vec![
    ///     WebSocketChannel::FuturesBtcUsdIndex,
    ///     WebSocketChannel::FuturesBtcUsdLastPrice
    /// ]).await?;
    ///
    /// assert_eq!(ws.subscriptions().await.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    async fn subscribe(&self, channels: Vec<WebSocketChannel>) -> Result<()>;

    /// Unsubscribes from the specified WebSocket channels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WebSocketChannel;
    /// let ws = ws_api.connect().await?;
    ///
    /// ws.subscribe(vec![WebSocketChannel::FuturesBtcUsdIndex]).await?;
    /// ws.unsubscribe(vec![WebSocketChannel::FuturesBtcUsdIndex]).await?;
    ///
    /// assert!(ws.subscriptions().await.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    async fn unsubscribe(&self, channels: Vec<WebSocketChannel>) -> Result<()>;

    /// Returns the set of currently subscribed channels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WebSocketChannel;
    /// let ws = ws_api.connect().await?;
    ///
    /// ws.subscribe(vec![
    ///     WebSocketChannel::FuturesBtcUsdIndex,
    ///     WebSocketChannel::FuturesBtcUsdLastPrice
    /// ]).await?;
    ///
    /// assert_eq!(ws.subscriptions().await.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    async fn subscriptions(&self) -> HashSet<WebSocketChannel>;

    /// Creates a new receiver for WebSocket updates.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WebSocketChannel;
    /// let ws = ws_api.connect().await?;
    ///
    /// let mut ws_rx = ws.receiver().await?;
    ///
    /// ws.subscribe(vec![WebSocketChannel::FuturesBtcUsdIndex]).await?;
    ///
    /// while let Ok(ws_update) = ws_rx.recv().await {
    ///     // Handle `ws_update`
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn receiver(&self) -> Result<Receiver<WebSocketUpdate>>;

    /// Disconnects the WebSocket. After disconnection, receivers stop receiving updates and the
    /// connection status changes to `Disconnected`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(ws_api: lnm_sdk::api_v2::WebSocketClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::WsConnectionStatus;
    /// let ws = ws_api.connect().await?;
    ///
    /// assert!(matches!(ws.connection_status().await, WsConnectionStatus::Connected));
    ///
    /// ws.disconnect().await?;
    ///
    /// assert!(matches!(
    ///     ws.connection_status().await,
    ///     WsConnectionStatus::Disconnected
    /// ));
    /// # Ok(())
    /// # }
    /// ```
    async fn disconnect(&self) -> Result<()>;
}
