use chrono::{DateTime, Utc};
use std::result;
use thiserror::Error;

use lnm_sdk::api::rest::error::RestApiError;

use crate::db::error::DbError;

#[derive(Error, Debug)]
pub enum SyncPriceHistoryError {
    #[error("RestApiMaxTrialsReached error: error {error}, trials {trials}")]
    RestApiMaxTrialsReached { error: RestApiError, trials: u32 },
    #[error("PriceEntriesUnsorted error")]
    PriceEntriesUnsorted,
    #[error("PriceEntriesWithoutOverlap error")]
    PriceEntriesWithoutOverlap,
    #[error("FromObservedTimeNotReceived error: {0}")]
    FromObservedTimeNotReceived(DateTime<Utc>),
    #[error("HistoryUpdateHandlerFailed error")]
    HistoryUpdateHandlerFailed,
    #[error("NoGapEntriesReceived error")]
    NoGapEntriesReceived,
    #[error("Database error: {0}")]
    Db(#[from] DbError),
    #[error("UnreachableDbGap error: gap {gap}, reach {reach}")]
    UnreachableDbGap {
        gap: DateTime<Utc>,
        reach: DateTime<Utc>,
    },
}

pub type Result<T> = result::Result<T, SyncPriceHistoryError>;
