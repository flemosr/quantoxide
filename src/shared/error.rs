use thiserror::Error;

use super::{MinIterationInterval, Period};

#[derive(Error, Debug)]
pub enum PeriodValidationError {
    #[error("Invalid period, must be at least {} candles", Period::MIN)]
    TooShort,

    #[error("Invalid period, must be at most {} candles", Period::MAX)]
    TooLong,
}

#[derive(Error, Debug)]
pub enum MinIterationIntervalValidationError {
    #[error(
        "Invalid minimum iteration interval, must be at least {}",
        MinIterationInterval::MIN
    )]
    InvalidMinIterationIntervalTooShort,

    #[error(
        "Invalid minimum iteration interval, must be at most {}",
        MinIterationInterval::MAX
    )]
    InvalidMinIterationIntervalTooLong,
}
