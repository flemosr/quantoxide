use async_trait::async_trait;
use std::collections::HashSet;

use super::{
    error::Result,
    manager::ResponseReceiver,
    models::{ConnectionState, LnmWebSocketChannel},
};

#[async_trait]
pub trait WebSocketRepository: Send + Sync {
    fn is_connected(&self) -> bool;

    async fn connection_state(&self) -> ConnectionState;

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()>;

    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel>;

    async fn receiver(&self) -> Result<ResponseReceiver>;

    async fn shutdown(&self) -> Result<()>;
}
