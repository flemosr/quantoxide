use std::result;

use thiserror::Error;

use super::{
    backtest::error::{BacktestError, SimulatedTradeExecutorError},
    live::{error::LiveError, executor::error::LiveTradeExecutorError},
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
pub enum TradeError {
    #[error("[Backtest] {0}")]
    Backtest(#[from] BacktestError),

    #[error("[Live] {0}")]
    Live(#[from] LiveError),

    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TradeError>;
