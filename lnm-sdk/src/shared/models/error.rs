use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuantityValidationError {
    #[error("Quantity must be at least 1")]
    TooLow,

    #[error("Quantity must be less than or equal to 500,000")]
    TooHigh,
}
