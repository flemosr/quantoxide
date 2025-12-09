use thiserror::Error;

use super::{
    leverage::Leverage,
    margin::Margin,
    price::{BoundedPercentage, LowerBoundedPercentage, Price},
    quantity::Quantity,
};

#[derive(Debug, Error)]
pub enum QuantityValidationError {
    #[error("Quantity must be at least {}. Value: {value}", Quantity::MIN)]
    TooLow { value: u64 },

    #[error(
        "Quantity must be less than or equal to {}. Value: {value}",
        Quantity::MAX
    )]
    TooHigh { value: u64 },

    #[error("Quantity must be an integer. Value: {value}")]
    NotAnInteger { value: f64 },
}

#[derive(Debug, Error)]
pub enum MarginValidationError {
    #[error("Margin must be at least 1")]
    TooLow,

    #[error("Margin must be a finite number")]
    NotFinite,

    #[error("Margin must be an integer")]
    NotAnInteger,
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
