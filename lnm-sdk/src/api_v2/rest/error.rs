use thiserror::Error;

use super::models::error::FuturesTradeRequestValidationError;

#[derive(Error, Debug)]
pub enum RestApiV2Error {
    #[error("Invalid futures trade request error: {0}")]
    FuturesTradeRequestValidation(FuturesTradeRequestValidationError),
}
