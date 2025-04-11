use std::result;
use thiserror::Error;

use crate::api::websocket::error::WebSocketApiError;

#[derive(Error, Debug)]
pub enum RealTimeCollectionError {
    #[error("WebSocketApiError error")]
    WebSocketApi(#[from] WebSocketApiError),
    #[error("RealTimeCollection generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, RealTimeCollectionError>;
