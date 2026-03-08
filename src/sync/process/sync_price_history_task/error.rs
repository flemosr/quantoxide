use std::{num::NonZeroU64, result};

use chrono::{DateTime, Utc};
use thiserror::Error;

use lnm_sdk::api_v3::error::RestApiError;

use crate::db::error::DbError;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SyncPriceHistoryRecoverableError {
    #[error("RestApiMaxTrialsReached error: error {error}, trials {trials}")]
    RestApiMaxTrialsReached {
        error: RestApiError,
        trials: NonZeroU64,
    },

    #[error("HistoryUpdateHandlerFailed error")]
    HistoryUpdateHandlerFailed,

    #[error("[Db] {0}")]
    Db(DbError),
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SyncPriceHistoryFatalError {
    #[error("API candles must have times rounded to the minute (no seconds/nanoseconds)")]
    ApiCandlesTimesNotRoundedToMinute,

    #[error("API candles must be ordered by time desc. Inconsistency at: {inconsistency_at}")]
    ApíCandlesNotOrderedByTimeDesc { inconsistency_at: DateTime<Utc> },

    #[error("API candles before {history_start} are not available")]
    ApiCandlesNotAvailableBeforeHistoryStart { history_start: DateTime<Utc> },

    #[error("Unreachable gap detected in the database. Gap at {gap}, configured reach at {reach}")]
    UnreachableDbGap {
        gap: DateTime<Utc>,
        reach: DateTime<Utc>,
    },

    #[error("Price history state `range_from` ({range_from}) can't be gte `range_to` ({range_to})")]
    InvalidPriceHistoryStateRange {
        range_from: DateTime<Utc>,
        range_to: DateTime<Utc>,
    },

    #[error("Price history state `reach` was not set, and it is required to evaluate DB gaps")]
    PriceHistoryStateReachNotSet,

    #[error(
        "Lookback reach ({lookback_reach}) is before the configured price history reach ({price_history_reach})"
    )]
    LookbackExceedsPriceHistoryReach {
        lookback_reach: DateTime<Utc>,
        price_history_reach: DateTime<Utc>,
    },
}

#[derive(Error, Debug)]
pub enum SyncPriceHistoryError {
    #[error(transparent)]
    Recoverable(#[from] SyncPriceHistoryRecoverableError),

    #[error(transparent)]
    Fatal(#[from] SyncPriceHistoryFatalError),
}

impl From<DbError> for SyncPriceHistoryError {
    fn from(e: DbError) -> Self {
        SyncPriceHistoryRecoverableError::Db(e).into()
    }
}

pub(super) type Result<T> = result::Result<T, SyncPriceHistoryError>;
