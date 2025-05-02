use std::result;
use thiserror::Error;

use lnm_sdk::api::rest::models::{
    Price,
    error::{MarginValidationError, PriceValidationError},
};

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("[MarginValidation] {0}")]
    MarginValidation(#[from] MarginValidationError),

    #[error("[PriceValidation] {0}")]
    PriceValidation(#[from] PriceValidationError),

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

    #[error("Error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, SimulationError>;
