use thiserror::Error;

use crate::shared::models::error::{
    LeverageValidationError, MarginValidationError, PriceValidationError, QuantityValidationError,
    TradeValidationError,
};

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

    #[error("[TradeValidation] {0}")]
    TradeValidation(#[from] TradeValidationError),
}
