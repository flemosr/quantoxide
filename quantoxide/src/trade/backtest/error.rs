use std::result;

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use tokio::task::JoinError;

use crate::{
    db::error::DbError, shared::LookbackPeriod,
    signal::process::error::SignalProcessRecoverableError,
    sync::process::sync_price_history_task::error::SyncPriceHistoryError,
};

use super::{
    super::error::{TradeCoreError, TradeExecutorError},
    executor::error::SimulatedTradeExecutorError,
};

#[derive(Error, Debug)]
pub enum BacktestError {
    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("Backtest process was already consumed")]
    ProcessAlreadyConsumed,

    #[error("Buffer size must be at least 100, got {size}")]
    InvalidConfigurationBufferSize { size: usize },

    #[error("Maximum running quantity must be at least 1, got {max}")]
    InvalidConfigurationMaxRunningQtd { max: usize },

    #[error(
        "Start and end times must be rounded to minutes. Start time: {start_time}, end time: {end_time}"
    )]
    InvalidTimeRangeNotRounded {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    },

    #[error(
        "Start time must be before the end time. Start time: {start_time}, end time: {end_time}"
    )]
    InvalidTimeRangeSequence {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    },

    #[error("Backtest duration must be at least {min_duration} day, got {duration_hours} hours")]
    InvalidTimeRangeTooShort {
        min_duration: Duration,
        duration_hours: i64,
    },

    #[error("Buffer size {buffer_size} is incompatible with max lookback {max_lookback}")]
    IncompatibleBufferSize {
        buffer_size: usize,
        max_lookback: LookbackPeriod,
    },

    #[error("Price History State Evaluation error: {0}")]
    PriceHistoryStateEvaluation(SyncPriceHistoryError),

    #[error("No price history entries found before start time")]
    DatabaseNoEntriesBeforeStartTime,

    #[error(
        "Required price history range including lookback period ({lookback_time} to {end_time}) is not available"
    )]
    PriceHistoryUnavailable {
        lookback_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    },

    #[error("Buffer date calculation resulted in out of range value")]
    DateRangeBufferOutOfRange,

    #[error("Set trade executor error: {0}")]
    SetTradeExecutor(TradeCoreError),

    #[error("Operator error: {0}")]
    OperatorError(TradeCoreError),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("Signal evaluation error: {0}")]
    SignalEvaluationError(SignalProcessRecoverableError),

    #[error("Signal processing error: {0}")]
    SignalProcessingError(TradeCoreError),

    #[error("Trade executor tick update error: {0}")]
    ExecutorTickUpdate(SimulatedTradeExecutorError),

    #[error("Trade executor time update error: {0}")]
    ExecutorTimeUpdate(SimulatedTradeExecutorError),

    #[error("Trade executor state evaluation error: {0}")]
    ExecutorStateEvaluation(TradeExecutorError),
}

pub(super) type Result<T> = result::Result<T, BacktestError>;
