use async_trait::async_trait;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::broadcast::Receiver;

use super::{
    error::Result,
    models::{LnmWebSocketChannel, WebSocketUpdate},
    state::ConnectionStatus,
};

#[async_trait]
pub trait WebSocketRepository: Send + Sync {
    async fn is_connected(&self) -> bool;

    async fn connection_status(&self) -> Arc<ConnectionStatus>;

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel>;

    async fn receiver(&self) -> Result<Receiver<WebSocketUpdate>>;

    async fn disconnect(&self) -> Result<()>;
}
