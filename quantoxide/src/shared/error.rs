use thiserror::Error;

use super::{LookbackPeriod, MinIterationInterval};

#[derive(Error, Debug)]
pub enum LookbackPeriodValidationError {
    #[error("Invalid lookback period, must be at least {}", LookbackPeriod::MIN)]
    InvalidLookbackPeriodTooShort,

    #[error("Invalid lookback period, must be at most {}", LookbackPeriod::MAX)]
    InvalidLookbackPeriodTooLong,
}


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

