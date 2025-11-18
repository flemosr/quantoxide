use thiserror::Error;

use super::models::error::{
    FuturesCrossTradeOrderValidationError, FuturesIsolatedTradeRequestValidationError,
};

#[derive(Error, Debug)]
pub enum RestApiV3Error {
    #[error("Invalid futures isolated trade request error: {0}")]
    FuturesIsolatedTradeRequestValidation(FuturesIsolatedTradeRequestValidationError),

    #[error("Invalid futures cross place order error: {0}")]
    FuturesCrossTradeOrderValidation(FuturesCrossTradeOrderValidationError),
}
