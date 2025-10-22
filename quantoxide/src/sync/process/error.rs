use std::result;

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use super::{RealTimeCollectionError, SyncPriceHistoryError};

#[derive(Error, Debug)]
pub enum SyncProcessRecoverableError {
    #[error("[SyncPriceHistory] {0}")]
    SyncPriceHistory(#[from] SyncPriceHistoryError),

    #[error("[RealTimeCollection] {0}")]
    RealTimeCollection(#[from] RealTimeCollectionError),

    #[error("[RealTimeCollectionTaskJoin] {0}")]
    RealTimeCollectionTaskJoin(JoinError),

    #[error("UnexpectedRealTimeCollectionShutdown error")]
    UnexpectedRealTimeCollectionShutdown,

    #[error("PriceTickRecv error: {0}")]
    PriceTickRecv(RecvError),
}

#[derive(Error, Debug)]
pub enum SyncProcessFatalError {
    #[error("Shutdown signal channel recv error: {0}")]
    ShutdownSignalRecv(RecvError),

    #[error("[SyncProcessTaskJoin] {0}")]
    SyncProcessTaskJoin(JoinError),

    #[error("Failed to send sync shutdown signal error: {0}")]
    SendShutdownSignalFailed(SendError<()>),

    #[error("Sync shutdown timeout error")]
    ShutdownTimeout,
}

#[derive(Error, Debug)]
pub enum SyncProcessError {
    #[error(transparent)]
    Recoverable(#[from] SyncProcessRecoverableError),

    #[error(transparent)]
    Fatal(#[from] SyncProcessFatalError),
}

pub type Result<T> = result::Result<T, SyncProcessError>;
