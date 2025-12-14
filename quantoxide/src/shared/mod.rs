use std::{fmt, result::Result};

use chrono::Duration;

pub mod error;

use error::{LookbackPeriodValidationError, MinIterationIntervalValidationError};

/// Validated lookback period specifying how much historical data to provide for analysis.
///
/// Represents a duration in minutes with enforced minimum and maximum bounds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct LookbackPeriod(u64);

impl LookbackPeriod {
    pub const MIN: Self = Self(5);

    pub const MAX: Self = Self(1440);

    /// Returns the lookback period as a [`Duration`].
    pub fn as_duration(&self) -> Duration {
        Duration::minutes(self.0 as i64)
    }

    /// Returns the lookback period in minutes as a `u64`.
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the lookback period in minutes as a `usize`.
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Returns the lookback period in minutes as an `i64`.
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

/// Validated minimum interval between successive iterations.
///
/// Represents a duration with enforced bounds to prevent iterations from running too frequently or
/// too infrequently.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct MinIterationInterval(Duration);

impl MinIterationInterval {
    pub const MIN: Self = Self(Duration::seconds(5));

    pub const MAX: Self = Self(Duration::minutes(10));

    pub fn seconds(secs: u64) -> Result<Self, MinIterationIntervalValidationError> {
        Self::try_from(Duration::seconds(secs as i64))
    }

    /// Returns the minimum iteration interval as a [`Duration`].
    pub fn as_duration(&self) -> Duration {
        self.0
    }
}

impl TryFrom<Duration> for MinIterationInterval {
    type Error = MinIterationIntervalValidationError;

    fn try_from(value: Duration) -> Result<Self, Self::Error> {
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
