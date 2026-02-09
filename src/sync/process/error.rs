use std::{result, time::Duration};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use super::{
    real_time_collection_task::error::RealTimeCollectionError,
    sync_price_history_task::error::SyncPriceHistoryError,
};

#[derive(Error, Debug)]
#[non_exhaustive]
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

    #[error("Maximum interval between price ticks ({0:?}) was exceeded.")]
    MaxPriceTickIntevalExceeded(Duration),
}

#[derive(Error, Debug)]
#[non_exhaustive]
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

pub(super) type Result<T> = result::Result<T, SyncProcessError>;
