use thiserror::Error;

use super::price::{BoundedPercentage, LowerBoundedPercentage, Price};

#[derive(Debug, Error)]
pub enum QuantityValidationError {
    #[error("Quantity must be at least 1")]
    TooLow,

    #[error("Quantity must be less than or equal to 500,000")]
    TooHigh,
}

#[derive(Debug, Error)]
pub enum LeverageValidationError {
    #[error("Leverage must be at least 1")]
    TooLow,

    #[error("Leverage must be at most 100")]
    TooHigh,
}

#[derive(Debug, Error)]
pub enum BoundedPercentageValidationError {
    #[error(
        "BoundedPercentage must be at least {}. Value: {value}",
        BoundedPercentage::MIN
    )]
    BelowMinimum { value: f64 },

    #[error(
        "BoundedPercentage must be at most {}. Value: {value}",
        BoundedPercentage::MAX
    )]
    AboveMaximum { value: f64 },
}

#[derive(Debug, Error)]
pub enum LowerBoundedPercentageValidationError {
    #[error(
        "LowerBoundedPercentage must be at least {}. Value: {value}",
        LowerBoundedPercentage::MIN
    )]
    BelowMinimum { value: f64 },

    #[error("LowerBoundedPercentage must be a finite number")]
    NotFinite,
}

#[derive(Debug, Error)]
pub enum PriceValidationError {
    #[error("Price must be at least {}. Value: {value}", Price::MIN)]
    TooLow { value: f64 },

    #[error("Price must be a multiple of 0.5. Value: {value}")]
    NotMultipleOfTick { value: f64 },

    #[error("Price must be at most {}. Value: {value}", Price::MAX)]
    TooHigh { value: f64 },
}
