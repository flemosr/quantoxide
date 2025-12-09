use thiserror::Error;

use crate::{api_v3::models::CrossLeverage, shared::models::error::QuantityValidationError};

#[derive(Debug, Error)]
pub enum CrossLeverageValidationError {
    #[error(
        "CrossLeverage must be at least {}. Value: {value}",
        CrossLeverage::MIN
    )]
    TooLow { value: u64 },

    #[error(
        "CrossLeverage must be less than or equal to {}. Value: {value}",
        CrossLeverage::MAX
    )]
    TooHigh { value: u64 },

    #[error("CrossLeverage must be an integer. Value: {value}")]
    NotAnInteger { value: f64 },
}

#[derive(Debug, Error)]
pub enum FuturesIsolatedTradeRequestValidationError {
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

    #[error("Client Id is too long")]
    ClientIdTooLong,
}

#[derive(Debug, Error)]
pub enum FuturesCrossTradeOrderValidationError {
    #[error("Client Id is too long")]
    ClientIdTooLong,
}
