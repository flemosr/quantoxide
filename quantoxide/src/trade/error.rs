use std::result;

use thiserror::Error;

use lnm_sdk::api::rest::models::error::PriceValidationError;

use super::{live::error::LiveTradeError, manager::error::SimulationError};

#[derive(Error, Debug)]
pub enum TradeError {
    #[error("RiskParamsConversion error {0}")]
    RiskParamsConversion(PriceValidationError),

    #[error("[Simulated] {0}")]
    Simulated(#[from] SimulationError),

    #[error("[Live] {0}")]
    Live(#[from] LiveTradeError),

    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TradeError>;
