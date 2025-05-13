use thiserror::Error;

use lnm_sdk::api::rest::{error::RestApiError, models::error::QuantityValidationError};

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("Generic error, {0}")]
    Generic(String),
}
