use thiserror::Error;

use super::models::error::FuturesIsolatedTradeRequestValidationError;

#[derive(Error, Debug)]
pub enum RestApiV3Error {
    #[error("Invalid futures isolated trade request error: {0}")]
    FuturesIsolatedTradeRequestValidation(FuturesIsolatedTradeRequestValidationError),
}
