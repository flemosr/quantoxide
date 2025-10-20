use std::result;

use thiserror::Error;

use super::{
    backtest::error::SimulatedTradeExecutorError, live::executor::error::LiveTradeExecutorError,
};

#[derive(Error, Debug)]
pub enum TradeExecutorError {
    #[error("[Simulated] {0}")]
    Simulated(#[from] SimulatedTradeExecutorError),

    #[error("[Live] {0}")]
    Live(#[from] LiveTradeExecutorError),
}

pub type TradeExecutorResult<T> = result::Result<T, TradeExecutorError>;

#[derive(Error, Debug)]
pub enum TradeCoreError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type TradeCoreResult<T> = result::Result<T, TradeCoreError>;
