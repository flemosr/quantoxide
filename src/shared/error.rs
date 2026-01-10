use chrono::Duration;
use thiserror::Error;

use super::{MinIterationInterval, Period};

#[derive(Error, Debug)]
pub enum PeriodValidationError {
    #[error(
        "Invalid period, must be at least {} candles. Value: {value}",
        Period::MIN
    )]
    TooShort { value: u64 },

    #[error(
        "Invalid period, must be at most {} candles. Value: {value}",
        Period::MAX
    )]
    TooLong { value: u64 },
}

#[derive(Error, Debug)]
pub enum MinIterationIntervalValidationError {
    #[error(
        "Invalid minimum iteration interval, must be at least {}. Value: {}s",
        MinIterationInterval::MIN,
        value.num_seconds()
    )]
    TooShort { value: Duration },

    #[error(
        "Invalid minimum iteration interval, must be at most {}. Value: {}s",
        MinIterationInterval::MAX,
        value.num_seconds()
    )]
    TooLong { value: Duration },
}
