use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebSocketApiError {
    #[error("WebSocket generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, WebSocketApiError>;
