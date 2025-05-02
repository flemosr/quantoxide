use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoundedPercentageValidationError {
    #[error("Percentage must be at least 0.1%")]
    BelowMinimum,

    #[error("Percentage must be at most 99.9%")]
    AboveMaximum,

    #[error("Percentage must be a finite number")]
    NotFinite,
}

#[derive(Debug, Error)]
pub enum PriceValidationError {
    #[error("Price must be at least 1")]
    AtLeastOne,

    #[error("Price must be a multiple of 0.5")]
    NotMultipleOfTick,

    #[error("Price must be a finite number")]
    NotFinite,

    #[error("Invalid percentage change")]
    InvalidPercentage,
}

#[derive(Debug, Error)]
pub enum LeverageValidationError {
    #[error("Leverage must be at least 1")]
    TooLow,

    #[error("Leverage must be at most 100")]
    TooHigh,

    #[error("Leverage must be a finite number")]
    NotFinite,
}

#[derive(Debug, Error)]
pub enum QuantityValidationError {
    #[error("Quantity must be positive")]
    NotPositive,

    #[error("Quantity must be at least 1")]
    TooLow,

    #[error("Quantity must be less than or equal to 500,000")]
    TooHigh,

    #[error("Quantity must be a finite number")]
    NotFinite,
}

#[derive(Debug, Error)]
pub enum MarginValidationError {
    #[error("Margin can't be negative")]
    Negative,

    #[error("Margin can't be zero")]
    Zero,

    #[error("Margin must be a finite number")]
    NotFinite,

    #[error("Margin must be an integer")]
    NotInteger,
}

#[derive(Debug, Error)]
pub enum FuturesTradeRequestValidationError {
    #[error("Price cannot be set for market orders")]
    PriceSetForMarketOrder,

    #[error("Price must be set for limit orders")]
    MissingPriceForLimitOrder,

    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("Stop loss must be lower than the entry price")]
    StopLossHigherThanPrice,

    #[error("Take profit must be higher than the entry price")]
    TakeProfitLowerThanPrice,
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("[PriceValidation] {0}")]
    PriceValidation(#[from] PriceValidationError),

    #[error("[LeverageValidation] {0}")]
    LeverageValidation(#[from] LeverageValidationError),

    #[error("[QuantityValidation] {0}")]
    QuantityValidation(#[from] QuantityValidationError),

    #[error("[MarginValidation] {0}")]
    MarginValidation(#[from] MarginValidationError),

    #[error("[FuturesTradeRequestValidation] {0}")]
    FuturesTradeRequestValidation(#[from] FuturesTradeRequestValidationError),
}
