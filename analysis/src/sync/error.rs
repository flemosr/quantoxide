use std::result;
use thiserror::Error;

use super::{
    real_time_collection_task::error::RealTimeCollectionError,
    sync_price_history_task::error::SyncPriceHistoryError,
};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("SyncPriceHistory error: {0}")]
    SyncPriceHistory(#[from] SyncPriceHistoryError),
    #[error("RealTimeCollection error: {0}")]
    RealTimeCollection(#[from] RealTimeCollectionError),
    #[error("SyncTransmiter failed error {0}")]
    SyncTransmiterFailed(String),
    #[error("TaskJoin error {0}")]
    TaskJoin(String),
    #[error("UnexpectedRealTimeCollectionShutdown error")]
    UnexpectedRealTimeCollectionShutdown,
}

pub type Result<T> = result::Result<T, SyncError>;
