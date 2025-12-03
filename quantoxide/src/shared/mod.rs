use std::fmt;

use chrono::Duration;

pub mod error;

use error::MinIterationIntervalValidationError;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct MinIterationInterval(Duration);

impl MinIterationInterval {
    pub const MIN: Self = Self(Duration::seconds(5));

    pub const MAX: Self = Self(Duration::minutes(10));

    pub fn as_duration(&self) -> Duration {
        self.0
    }
}

impl TryFrom<Duration> for MinIterationInterval {
    type Error = MinIterationIntervalValidationError;

    fn try_from(value: Duration) -> std::result::Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(MinIterationIntervalValidationError::InvalidMinIterationIntervalTooShort);
        }

        if value > Self::MAX.0 {
            return Err(MinIterationIntervalValidationError::InvalidMinIterationIntervalTooLong);
        }

        Ok(Self(value))
    }
}

impl fmt::Display for MinIterationInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
