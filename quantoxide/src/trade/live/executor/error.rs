use std::result;

use thiserror::Error;

use lnm_sdk::api::rest::error::RestApiError;

#[derive(Error, Debug)]
pub enum LiveTradeExecutorError {
    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("Balance is too high error")]
    BalanceTooHigh,

    // #[error("RiskParamsConversion error {0}")]
    // RiskParamsConversion(PriceValidationError),
    // #[error("TradeValidation error {0}")]
    // TradeValidation(TradeValidationError),
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type LiveTradeExecutorResult<T> = result::Result<T, LiveTradeExecutorError>;
