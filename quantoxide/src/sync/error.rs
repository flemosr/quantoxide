use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use super::{RealTimeCollectionError, SyncPriceHistoryError, SyncStatus};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("[SyncPriceHistory] {0}")]
    SyncPriceHistory(#[from] SyncPriceHistoryError),

    #[error("[RealTimeCollection] {0}")]
    RealTimeCollection(#[from] RealTimeCollectionError),

    #[error("[SyncTransmiterFailed] {0}")]
    SyncTransmiterFailed(SendError<Arc<SyncStatus>>),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("[ShutdownRecv] {0}")]
    ShutdownRecv(RecvError),

    #[error("UnexpectedRealTimeCollectionShutdown error")]
    UnexpectedRealTimeCollectionShutdown,

    #[error("PriceTickRecv error: {0}")]
    PriceTickRecv(RecvError),

    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Failed to send sync process shutdown request error: {0}")]
    SendShutdownFailed(SendError<()>),

    #[error("Sync shutdown timeout error")]
    ShutdownTimeout,
}

impl PartialEq for SyncError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for SyncError {}

pub type Result<T> = result::Result<T, SyncError>;
