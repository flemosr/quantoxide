use std::{result, sync::Arc};
use thiserror::Error;

use crate::api::websocket::{error::WebSocketApiError, models::ConnectionState};

#[derive(Error, Debug)]
pub enum RealTimeCollectionError {
    #[error("WebSocketApiError error")]
    WebSocketApi(#[from] WebSocketApiError),
    #[error("BadConnectionUpdate error, {0:?}")]
    BadConnectionUpdate(Arc<ConnectionState>),
    #[error("RealTimeCollection generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, RealTimeCollectionError>;
