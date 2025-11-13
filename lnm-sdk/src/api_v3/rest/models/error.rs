use thiserror::Error;

use crate::shared::models::error::QuantityValidationError;

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
