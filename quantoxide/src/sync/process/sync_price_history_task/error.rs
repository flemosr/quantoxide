use std::result;

use chrono::{DateTime, Duration, Utc};
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

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("UnreachableDbGap error: gap {gap}, reach {reach}")]
    UnreachableDbGap {
        gap: DateTime<Utc>,
        reach: DateTime<Utc>,
    },

    #[error("Live range {range} must be lte `sync_history_reach` {sync_history_reach}")]
    InvalidLiveRange {
        range: Duration,
        sync_history_reach: Duration,
    },

    #[error("Price history state `range_from` ({range_from}) can't be gte `range_to` ({range_to})")]
    InvalidPriceHistoryStateRange {
        range_from: DateTime<Utc>,
        range_to: DateTime<Utc>,
    },

    #[error("Price history state `reach` was not set, and it is required to evaluate DB gaps")]
    PriceHistoryStateReachNotSet,
}

pub type Result<T> = result::Result<T, SyncPriceHistoryError>;
