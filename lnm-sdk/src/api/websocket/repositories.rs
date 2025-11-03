use std::collections::HashSet;

use async_trait::async_trait;
use tokio::sync::broadcast::Receiver;

use super::{
    error::Result,
    models::{LnmWebSocketChannel, WebSocketUpdate},
    state::WsConnectionStatus,
};

#[async_trait]
pub trait WebSocketRepository: Send + Sync {
    async fn is_connected(&self) -> bool;

    async fn connection_status(&self) -> WsConnectionStatus;

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel>;

    async fn receiver(&self) -> Result<Receiver<WebSocketUpdate>>;

    async fn disconnect(&self) -> Result<()>;
}
