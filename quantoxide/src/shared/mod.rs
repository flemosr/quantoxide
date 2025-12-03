use std::fmt;

use chrono::Duration;

pub mod error;

use error::{LookbackPeriodValidationError, MinIterationIntervalValidationError};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct LookbackPeriod(u64);

impl LookbackPeriod {
    pub const MIN: Self = Self(5);

    pub const MAX: Self = Self(1440);

    pub fn as_duration(&self) -> Duration {
        Duration::minutes(self.0 as i64)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub fn as_i64(&self) -> i64 {
        self.0 as i64
    }
}

impl TryFrom<u64> for LookbackPeriod {
    type Error = LookbackPeriodValidationError;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(LookbackPeriodValidationError::InvalidLookbackPeriodTooShort);
        }

        if value > Self::MAX.0 {
            return Err(LookbackPeriodValidationError::InvalidLookbackPeriodTooLong);
        }

        Ok(Self(value))
    }
}

impl fmt::Display for LookbackPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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
