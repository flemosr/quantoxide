use thiserror::Error;

use super::MinIterationInterval;

#[derive(Error, Debug)]
pub enum MinIterationIntervalValidationError {
    #[error(
        "Invalid minimum iteration interval, must be at least {}",
        MinIterationInterval::MIN
    )]
    InvalidMinIterationIntervalTooShort,

    #[error(
        "Invalid lookback period, must be at most {}",
        MinIterationInterval::MAX
    )]
    InvalidMinIterationIntervalTooLong,
}
