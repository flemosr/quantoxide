use async_trait::async_trait;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::broadcast::Receiver;

use super::{
    error::Result,
    models::{ConnectionState, LnmWebSocketChannel, WebSocketApiRes},
};

#[async_trait]
pub trait WebSocketRepository: Send + Sync {
    async fn is_connected(&self) -> bool;

    async fn connection_state(&self) -> Arc<ConnectionState>;

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel>;

    async fn receiver(&self) -> Result<Receiver<WebSocketApiRes>>;

    async fn disconnect(&self) -> Result<()>;
}
