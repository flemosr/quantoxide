use std::result;

use thiserror::Error;
use tokio::task::JoinError;

use lnm_sdk::api::rest::{error::RestApiError, models::error::QuantityValidationError};

#[derive(Error, Debug)]
pub enum LiveTradeError {
    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for LiveTradeError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for LiveTradeError {}

pub type Result<T> = result::Result<T, LiveTradeError>;
