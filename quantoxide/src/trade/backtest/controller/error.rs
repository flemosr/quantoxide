use std::result;

use chrono::{DateTime, Utc};
use thiserror::Error;

use lnm_sdk::api::rest::models::{
    Price, TradeSide,
    error::{MarginValidationError, PriceValidationError},
};

use crate::db::{error::DbError, models::PriceHistoryEntry};

#[derive(Error, Debug)]
pub enum SimulatedTradeControllerError {
    #[error("[MarginValidation] {0}")]
    MarginValidation(#[from] MarginValidationError),

    #[error("[PriceValidation] {0}")]
    PriceValidation(#[from] PriceValidationError),

    #[error(
        "Stoploss ({stoploss}) can't be below liquidation price ({liquidation}) for long positions"
    )]
    StoplossBelowLiquidationLong { stoploss: Price, liquidation: Price },

    #[error("Stoploss ({stoploss}) can't be above entry price ({entry_price}) for long positions")]
    StoplossAboveEntryForLong { stoploss: Price, entry_price: Price },

    #[error(
        "Takeprofit ({takeprofit}) can't be below entry price ({entry_price}) for long positions"
    )]
    TakeprofitBelowEntryForLong {
        takeprofit: Price,
        entry_price: Price,
    },

    #[error(
        "Stoploss ({stoploss}) can't be above liquidation price ({liquidation}) for short positions"
    )]
    StoplossAboveLiquidationShort { stoploss: Price, liquidation: Price },

    #[error("Stoploss ({stoploss}) can't be below entry price ({entry_price}) for short positions")]
    StoplossBelowEntryForShort { stoploss: Price, entry_price: Price },

    #[error(
        "Takeprofit ({takeprofit}) can't be above entry price ({entry_price}) for short positions"
    )]
    TakeprofitAboveEntryForShort {
        takeprofit: Price,
        entry_price: Price,
    },

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

pub type Result<T> = result::Result<T, SimulatedTradeControllerError>;
