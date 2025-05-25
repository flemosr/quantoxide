use std::result;

use thiserror::Error;

use lnm_sdk::api::rest::models::error::PriceValidationError;

use super::{
    backtest::error::{BacktestError, SimulatedTradeControllerError},
    live::error::LiveTradeError,
};

#[derive(Error, Debug)]
pub enum TradeError {
    #[error("RiskParamsConversion error {0}")]
    RiskParamsConversion(PriceValidationError),

    #[error("[Backtest] {0}")]
    Backtest(#[from] BacktestError),

    #[error("[Live] {0}")]
    Live(#[from] LiveTradeError),

    #[error("Generic error, {0}")]
    Generic(String),
}

impl From<SimulatedTradeControllerError> for TradeError {
    fn from(value: SimulatedTradeControllerError) -> Self {
        Self::Backtest(BacktestError::Manager(value))
    }
}

pub type Result<T> = result::Result<T, TradeError>;
