use std::result;

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use lnm_sdk::rest::v3::error::{
    CrossExposureValidationError, CrossQuantityValidationError, PriceValidationError,
    TradeValidationError,
};

use crate::db::error::DbError;

use super::super::super::error::TradeCoreError;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SimulatedTradeExecutorError {
    #[error("[InvalidMarketPrice] {0}")]
    InvalidMarketPrice(PriceValidationError),

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

    #[error("Tick update failed, price validation error: {0}")]
    TickUpdatePriceValidation(PriceValidationError),

    #[error("TradeValidation error {0}")]
    TradeValidation(TradeValidationError),

    #[error("CrossQuantityValidation error {0}")]
    CrossQuantityValidation(CrossQuantityValidationError),

    #[error("CrossExposureValidation error {0}")]
    CrossExposureValidation(CrossExposureValidationError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("Balance is too high error")]
    BalanceTooHigh,

    #[error("Cross-position numeric overflow")]
    CrossPositionOverflow,

    #[error("Cross free margin is too low error")]
    CrossFreeMarginTooLow,

    #[error("Cross position not coherent (illiquid or free margin exhausted)")]
    CrossPositionIncoherent,

    #[error("Trade {trade_id} is not running")]
    TradeNotRunning { trade_id: Uuid },

    #[error("Price Trigger update error")]
    PriceTriggerUpdate(TradeCoreError),

    #[error("Stoploss evaluation error")]
    StoplossEvaluation(TradeCoreError),

    #[error("Closed history update error")]
    ClosedHistoryUpdate(TradeCoreError),
}

pub type SimulatedTradeExecutorResult<T> = result::Result<T, SimulatedTradeExecutorError>;
