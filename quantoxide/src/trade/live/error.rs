use thiserror::Error;

use lnm_sdk::api::rest::models::error::QuantityValidationError;

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("Generic error, {0}")]
    Generic(String),
}
