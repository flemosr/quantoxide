use std::{result, sync::Arc};
use thiserror::Error;

use super::models::ConnectionState;

#[derive(Error, Debug)]
pub enum WebSocketApiError {
    #[error("BadConnectionState error")]
    BadConnectionState(Arc<ConnectionState>),
    #[error("WebSocket generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, WebSocketApiError>;
