use chrono::{DateTime, Utc};
use thiserror::Error;

use lnm_sdk::api::rest::models::{
    TradeSide,
    error::{PriceValidationError, TradeValidationError, ValidationError},
};

use crate::db::{error::DbError, models::PriceHistoryEntry};

#[derive(Error, Debug)]
pub enum SimulatedTradeExecutorError {
    #[error("[PriceValidation] {0}")]
    PriceValidation(#[from] PriceValidationError),

    #[error("TradeValidation error {0}")]
    TradeValidation(#[from] TradeValidationError),

    #[error("[Validation] {0}")]
    ValidationError(#[from] ValidationError),

    #[error("Invalid time sequence: new time {new_time} is not after current time {current_time}")]
    TimeSequenceViolation {
        new_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
    },

    #[error("No price history entry found at or before {time}")]
    NoPriceHistoryEntry { time: DateTime<Utc> },

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("Max running trades ({max_qtd}) reached")]
    MaxRunningTradesReached { max_qtd: usize },

    #[error("Invalid trade state for price boundary check")]
    InvalidTradeBoundaryState {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        min: f64,
        max: f64,
        side: TradeSide,
        entry: PriceHistoryEntry,
    },

    #[error("Generic error, {0}")]
    Generic(String),
}
