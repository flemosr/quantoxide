use std::result;

use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;

use lnm_sdk::{WsConnectionStatus, error::WebSocketApiError};

use crate::db::error::DbError;

#[derive(Error, Debug)]
pub enum RealTimeCollectionError {
    #[error("[WebSocketApi] {0}")]
    WebSocketApi(#[from] WebSocketApiError),

    #[error("BadConnectionUpdate error: {0}")]
    BadConnectionUpdate(WsConnectionStatus),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("RecvLagged error, skipped: {skipped}")]
    WebSocketRecvLagged { skipped: u64 },

    #[error("RecvLagged error")]
    WebSocketRecvClosed,

    #[error("Shutdown signal channel error: {0}")]
    ShutdownSignalRecv(RecvError),
}

pub type Result<T> = result::Result<T, RealTimeCollectionError>;
