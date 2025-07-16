use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use super::{RealTimeCollectionError, SyncPriceHistoryError, SyncState};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("[SyncPriceHistory] {0}")]
    SyncPriceHistory(#[from] SyncPriceHistoryError),

    #[error("[RealTimeCollection] {0}")]
    RealTimeCollection(#[from] RealTimeCollectionError),

    #[error("[SyncTransmiterFailed] {0}")]
    SyncTransmiterFailed(SendError<Arc<SyncState>>),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("[ShutdownRecv] {0}")]
    ShutdownRecv(RecvError),

    #[error("UnexpectedRealTimeCollectionShutdown error")]
    UnexpectedRealTimeCollectionShutdown,

    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for SyncError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for SyncError {}

pub type Result<T> = result::Result<T, SyncError>;
