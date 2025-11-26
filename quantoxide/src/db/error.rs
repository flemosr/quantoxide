use std::result;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::indicators::error::IndicatorError;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Connection error: {0}")]
    Connection(sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(sqlx::migrate::MigrateError),

    #[error("Query error: {0}")]
    Query(sqlx::Error),

    #[error("Transaction begin error: {0}")]
    TransactionBegin(sqlx::Error),

    #[error("Transaction commit error: {0}")]
    TransactionCommit(sqlx::Error),

    #[error("Unexpected query result: {0}")]
    UnexpectedQueryResult(String),

    #[error(
        "When adding entries, `next_observed_time` ({next_observed_time}) must be gt than first entry `time` ({first_entry_time})"
    )]
    NewEntriesInvalidNextObservedTime {
        next_observed_time: DateTime<Utc>,
        first_entry_time: DateTime<Utc>,
    },

    #[error("New entries must be sorted by time in descending order")]
    NewEntriesNotSortedTimeDescending,

    #[error(
        "There are no price history entries with time lte the start of the LOCF range ({start_locf_sec})"
    )]
    InvalidLocfRange { start_locf_sec: DateTime<Utc> },

    #[error("New DB candles must have times rounded to the minute (no seconds/nanoseconds)")]
    NewDbCandlesTimesNotRoundedToMinute,

    #[error("New DB candles must be ordered by time desc. Inconsistency at: {inconsistency_at}")]
    NewDbCandlesNotOrderedByTimeDesc { inconsistency_at: DateTime<Utc> },

    #[error("Attempted to update a stable candle at time {time}")]
    AttemptedToUpdateStableCandle { time: DateTime<Utc> },

    #[error(transparent)]
    IndicatorEvaluation(#[from] IndicatorError),
}

pub(super) type Result<T> = result::Result<T, DbError>;
