use std::result;

use thiserror::Error;
use tokio::{sync::broadcast::error::RecvError, task::JoinError};

use super::{RealTimeCollectionError, SyncPriceHistoryError};

#[derive(Error, Debug)]
pub enum SyncProcessError {
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

    #[error("Shutdown signal channel error: {0}")]
    ShutdownSignalRecv(RecvError), // Both lagged and closed are not recoverable
}

pub type Result<T> = result::Result<T, SyncProcessError>;
