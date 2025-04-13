use std::{result, sync::Arc};
use thiserror::Error;
use tokio::{sync::broadcast::error::SendError, task::JoinError};

use super::{
    real_time_collection_task::error::RealTimeCollectionError,
    sync_price_history_task::error::SyncPriceHistoryError, SyncState,
};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("SyncPriceHistory error: {0}")]
    SyncPriceHistory(#[from] SyncPriceHistoryError),
    #[error("RealTimeCollection error: {0}")]
    RealTimeCollection(#[from] RealTimeCollectionError),
    #[error("SyncTransmiter failed error {0}")]
    SyncTransmiterFailed(SendError<Arc<SyncState>>),
    #[error("TaskJoin error {0}")]
    TaskJoin(JoinError),
    #[error("UnexpectedRealTimeCollectionShutdown error")]
    UnexpectedRealTimeCollectionShutdown,
}

impl PartialEq for SyncError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for SyncError {}

pub type Result<T> = result::Result<T, SyncError>;
