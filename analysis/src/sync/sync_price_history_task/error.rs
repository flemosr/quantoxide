use chrono::{DateTime, Utc};
use std::result;
use thiserror::Error;

use crate::{api::error::ApiError, db::error::DbError};

#[derive(Error, Debug)]
pub enum SyncPriceHistoryError {
    #[error("ApiMaxTrialsReached error: api_error {api_error}, trials {trials}")]
    ApiMaxTrialsReached { api_error: ApiError, trials: u32 },
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
