use std::result;

use thiserror::Error;
use uuid::Uuid;

use lnm_sdk::api_v3::{
    error::PriceValidationError,
    models::{Percentage, PercentageCapped, Price},
};

use crate::util::PanicPayload;

use super::{
    backtest::executor::error::SimulatedTradeExecutorError,
    live::executor::error::ExecutorActionError,
};

#[derive(Error, Debug)]
pub enum TradeExecutorError {
    #[error("[Simulated] {0}")]
    Simulated(#[from] SimulatedTradeExecutorError),

    #[error("[Live] {0}")]
    Live(#[from] ExecutorActionError),
}

pub(super) type TradeExecutorResult<T> = result::Result<T, TradeExecutorError>;

#[derive(Error, Debug)]
pub enum TradeCoreError {
    #[error("Trade {trade_id} is not closed")]
    TradeNotClosed { trade_id: Uuid },

    #[error("Trailing stoploss {tsl} cannot be smaller than step size {tsl_step_size}")]
    InvalidStoplossSmallerThanTrailingStepSize {
        tsl: PercentageCapped,
        tsl_step_size: PercentageCapped,
    },

    #[error("Invalid price resulting from applying discount {discount} to price {price}: {e}")]
    InvalidPriceApplyDiscount {
        price: Price,
        discount: PercentageCapped,
        e: PriceValidationError,
    },

    #[error("Invalid price resulting from applying gain {gain} to price {price}: {e}")]
    InvalidPriceApplyGain {
        price: Price,
        gain: Percentage,
        e: PriceValidationError,
    },

    #[error("Invalid price resulting from rounding {price}: {e}")]
    InvalidPriceRounding { price: f64, e: PriceValidationError },

    #[error("`SignalOperator::set_trade_executor` panicked: {0}")]
    SignalOperatorSetTradeExecutorPanicked(PanicPayload),

    #[error("`SignalOperator::set_trade_executor` error: {0}")]
    SignalOperatorSetTradeExecutorError(String),

    #[error("`SignalOperator::process_signal` panicked: {0}")]
    SignalOperatorProcessSignalPanicked(PanicPayload),

    #[error("`SignalOperator::process_signal` error: {0}")]
    SignalOperatorProcessSignalError(String),

    #[error("`RawOperator::set_trade_executor` panicked: {0}")]
    RawOperatorSetTradeExecutorPanicked(PanicPayload),

    #[error("`RawOperator::set_trade_executor` error: {0}")]
    RawOperatorSetTradeExecutorError(String),

    #[error("`RawOperator::lookback` panicked: {0}")]
    RawOperatorLookbackPanicked(PanicPayload),

    #[error("`RawOperator::min_iteration_interval` panicked: {0}")]
    RawOperatorMinIterationIntervalPanicked(PanicPayload),

    #[error("`RawOperator::iterate` panicked: {0}")]
    RawOperatorIteratePanicked(PanicPayload),

    #[error("`RawOperator::iterate` error: {0}")]
    RawOperatorIterateError(String),

    #[error("Tried to evaluate next update trigger of trade {trade_id} without stoploss")]
    NoNextTriggerTradeStoplossNotSet { trade_id: Uuid },
}

pub(super) type TradeCoreResult<T> = result::Result<T, TradeCoreError>;
