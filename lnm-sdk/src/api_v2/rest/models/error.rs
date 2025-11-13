use thiserror::Error;

use crate::shared::models::{
    error::{LeverageValidationError, PriceValidationError, QuantityValidationError},
    price::Price,
};

#[derive(Debug, Error)]
pub enum MarginValidationError {
    #[error("Margin must be at least 1")]
    TooLow,

    #[error("Margin must be a finite number")]
    NotFinite,
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
pub enum TradeValidationError {
    #[error(
        "Stoploss ({stoploss}) can't be below liquidation price ({liquidation}) for long positions"
    )]
    StoplossBelowLiquidationLong { stoploss: Price, liquidation: Price },

    #[error("Stoploss ({stoploss}) can't be above entry price ({entry_price}) for long positions")]
    StoplossAboveEntryForLong { stoploss: Price, entry_price: Price },

    #[error(
        "Takeprofit ({takeprofit}) can't be below entry price ({entry_price}) for long positions"
    )]
    TakeprofitBelowEntryForLong {
        takeprofit: Price,
        entry_price: Price,
    },

    #[error(
        "Stoploss ({stoploss}) can't be above liquidation price ({liquidation}) for short positions"
    )]
    StoplossAboveLiquidationShort { stoploss: Price, liquidation: Price },

    #[error("Stoploss ({stoploss}) can't be below entry price ({entry_price}) for short positions")]
    StoplossBelowEntryForShort { stoploss: Price, entry_price: Price },

    #[error(
        "Takeprofit ({takeprofit}) can't be above entry price ({entry_price}) for short positions"
    )]
    TakeprofitAboveEntryForShort {
        takeprofit: Price,
        entry_price: Price,
    },

    #[error("Trade params result in invalid quantity: {0}")]
    TradeParamsInvalidQuantity(QuantityValidationError),

    #[error(
        "New stoploss ({new_stoploss}) must be below market price ({market_price}) for long positions"
    )]
    NewStoplossNotBelowMarketForLong {
        new_stoploss: Price,
        market_price: Price,
    },

    #[error(
        "New stoploss ({new_stoploss}) must be below takeprofit ({takeprofit}) for long positions"
    )]
    NewStoplossNotBelowTakeprofitForLong {
        new_stoploss: Price,
        takeprofit: Price,
    },

    #[error(
        "New stoploss ({new_stoploss}) must be above market price ({market_price}) for short positions"
    )]
    NewStoplossNotAboveMarketForShort {
        new_stoploss: Price,
        market_price: Price,
    },

    #[error(
        "New stoploss ({new_stoploss}) must be above takeprofit ({takeprofit}) for short positions"
    )]
    NewStoplossNotAboveTakeprofitForShort {
        new_stoploss: Price,
        takeprofit: Price,
    },

    #[error("Added margin results in invalid leverage: {0}")]
    AddedMarginInvalidLeverage(LeverageValidationError),

    #[error("Cash-in results in invalid margin: {0}")]
    CashInInvalidMargin(MarginValidationError),

    #[error("Cash-in results in invalid leverage: {0}")]
    CashInInvalidLeverage(LeverageValidationError),

    #[error("Liquidation ({liquidation}) must be below price ({price}) for long positions")]
    LiquidationNotBelowPriceForLong { liquidation: Price, price: Price },

    #[error("Liquidation ({liquidation}) must be above price ({price}) for short positions")]
    LiquidationNotAbovePriceForShort { liquidation: Price, price: Price },
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
