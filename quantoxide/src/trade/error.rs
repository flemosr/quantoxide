use std::result;

use thiserror::Error;

use lnm_sdk::api::rest::models::error::{PriceValidationError, TradeValidationError};

use super::{
    backtest::error::{BacktestError, SimulatedTradeExecutorError},
    live::error::LiveError,
};

#[derive(Error, Debug)]
pub enum TradeError {
    // Consumer errors
    //
    #[error("RiskParamsConversion error {0}")]
    RiskParamsConversion(PriceValidationError),

    #[error("TradeValidation error {0}")]
    TradeValidation(TradeValidationError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("Balance is too high error")]
    BalanceTooHigh,

    // Other errors
    //
    #[error("[Backtest] {0}")]
    Backtest(#[from] BacktestError),

    #[error("[Live] {0}")]
    Live(#[from] LiveError),

    #[error("Generic error, {0}")]
    Generic(String),
}

impl From<SimulatedTradeExecutorError> for TradeError {
    fn from(value: SimulatedTradeExecutorError) -> Self {
        Self::Backtest(BacktestError::Manager(value))
    }
}

pub type Result<T> = result::Result<T, TradeError>;
