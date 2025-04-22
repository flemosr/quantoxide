#[derive(Debug, thiserror::Error)]
pub enum PriceValidationError {
    #[error("Price must be positive")]
    NotPositive,

    #[error("Price must be a multiple of 0.5")]
    NotMultipleOfTick,

    #[error("Price must be a finite number")]
    NotFinite,
}

#[derive(Debug, thiserror::Error)]
pub enum LeverageValidationError {
    #[error("Leverage must be at least 1")]
    TooLow,

    #[error("Leverage must be at most 100")]
    TooHigh,

    #[error("Leverage must be a finite number")]
    NotFinite,
}

#[derive(Debug, thiserror::Error)]
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

#[derive(Debug, thiserror::Error)]
pub enum MarginValidationError {
    #[error("Margin must be positive")]
    NotPositive,

    #[error("Margin must be at least 1")]
    TooLow,

    #[error("Margin must be a finite number")]
    NotFinite,
}

#[derive(Debug, thiserror::Error)]
pub enum FuturesTradeRequestValidationError {
    #[error("Either quantity or margin must be provided")]
    MissingQuantityAndMargin,

    #[error("Cannot provide both quantity and margin")]
    BothQuantityAndMarginProvided,

    #[error("Price cannot be set for market orders")]
    PriceSetForMarketOrder,

    #[error("Price must be set for limit orders")]
    MissingPriceForLimitOrder,

    #[error("Implied quantity must be valid")]
    InvalidImpliedQuantity(#[from] QuantityValidationError),

    #[error("Stop loss must be lower than the entry price")]
    StopLossHigherThanPrice,

    #[error("Take profit must be higher than the entry price")]
    TakeProfitLowerThanPrice,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error(transparent)]
    Price(#[from] PriceValidationError),

    #[error(transparent)]
    Leverage(#[from] LeverageValidationError),

    #[error(transparent)]
    Quantity(#[from] QuantityValidationError),

    #[error(transparent)]
    Margin(#[from] MarginValidationError),

    #[error(transparent)]
    FuturesTradeRequest(#[from] FuturesTradeRequestValidationError),
}
