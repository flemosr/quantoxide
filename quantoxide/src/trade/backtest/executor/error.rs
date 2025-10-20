use std::result;

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use lnm_sdk::api::rest::models::{
    TradeSide,
    error::{PriceValidationError, TradeValidationError},
};

use crate::{
    db::{error::DbError, models::PriceHistoryEntry},
    trade::error::TradeCoreError,
};

#[derive(Error, Debug)]
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

    #[error("Invalid trade state for price boundary check")]
    InvalidTradeBoundaryState {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        min: f64,
        max: f64,
        side: TradeSide,
        entry: PriceHistoryEntry,
    },

    #[error("Tick update failed, price validation error: {0}")]
    TickUpdatePriceValidation(PriceValidationError),

    #[error("TradeValidation error {0}")]
    TradeValidation(TradeValidationError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("Balance is too high error")]
    BalanceTooHigh,

    #[error("Trade {trade_id} is not running")]
    TradeNotRunning { trade_id: Uuid },

    #[error("Price Trigger update error")]
    PriceTriggerUpdate(TradeCoreError),

    #[error("Stoploss evaluation error")]
    StoplossEvaluation(TradeCoreError),
}

pub type SimulatedTradeExecutorResult<T> = result::Result<T, SimulatedTradeExecutorError>;
