use std::{num::NonZeroU64, result};

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;

use lnm_sdk::api_v2::error::RestApiError;

use crate::db::error::DbError;

#[derive(Error, Debug)]
pub enum SyncPriceHistoryError {
    #[error("Invalid period: from {from} to {to}")]
    InvalidPeriod {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },

    #[error("RestApiMaxTrialsReached error: error {error}, trials {trials}")]
    RestApiMaxTrialsReached {
        error: RestApiError,
        trials: NonZeroU64,
    },

    #[error("API candles must have times rounded to the minute (no seconds/nanoseconds)")]
    ApiCandlesTimesNotRoundedToMinute,

    #[error("API candles must be ordered by time desc. Inconsistency at: {inconsistency_at}")]
    ApíCandlesNotOrderedByTimeDesc { inconsistency_at: DateTime<Utc> },

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

pub(super) type Result<T> = result::Result<T, SyncPriceHistoryError>;
