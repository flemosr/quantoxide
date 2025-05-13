use std::result;

use lnm_sdk::api::rest::models::error::QuantityValidationError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, LiveError>;
