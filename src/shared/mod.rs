use std::{fmt, result::Result};

use chrono::Duration;

pub mod error;

use error::{MinIterationIntervalValidationError, PeriodValidationError};

/// Supported OHLC resolutions for trading operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OhlcResolution {
    OneMinute,
    ThreeMinutes,
    FiveMinutes,
    TenMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    FortyFiveMinutes,
    OneHour,
    TwoHours,
    ThreeHours,
    FourHours,
    OneDay,
}

impl OhlcResolution {
    /// Returns the resolution duration in minutes.
    pub const fn as_minutes(&self) -> u32 {
        match self {
            Self::OneMinute => 1,
            Self::ThreeMinutes => 3,
            Self::FiveMinutes => 5,
            Self::TenMinutes => 10,
            Self::FifteenMinutes => 15,
            Self::ThirtyMinutes => 30,
            Self::FortyFiveMinutes => 45,
            Self::OneHour => 60,
            Self::TwoHours => 120,
            Self::ThreeHours => 180,
            Self::FourHours => 240,
            Self::OneDay => 1440,
        }
    }

    /// Returns the resolution duration in seconds.
    pub const fn as_seconds(&self) -> u32 {
        self.as_minutes() * 60
    }
}

impl fmt::Display for OhlcResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneMinute => write!(f, "1m"),
            Self::ThreeMinutes => write!(f, "3m"),
            Self::FiveMinutes => write!(f, "5m"),
            Self::TenMinutes => write!(f, "10m"),
            Self::FifteenMinutes => write!(f, "15m"),
            Self::ThirtyMinutes => write!(f, "30m"),
            Self::FortyFiveMinutes => write!(f, "45m"),
            Self::OneHour => write!(f, "1h"),
            Self::TwoHours => write!(f, "2h"),
            Self::ThreeHours => write!(f, "3h"),
            Self::FourHours => write!(f, "4h"),
            Self::OneDay => write!(f, "1d"),
        }
    }
}

/// Validated period specifying how many candles of historical data to provide for analysis.
///
/// Represents a number of candles with enforced minimum and maximum bounds. The actual time span
/// depends on the candle resolution being used. For example, a period of 10 candles at 1-minute
/// resolution covers 10 minutes, while at 1-hour resolution it covers 10 hours.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct Period(u64);

impl Period {
    /// Minimum period: 5 candles.
    pub const MIN: Self = Self(5);

    /// Maximum period: 500 candles.
    pub const MAX: Self = Self(500);

    /// Returns the period as a [`Duration`] for the given resolution.
    ///
    /// This calculates the time span by multiplying the number of candles by the resolution's
    /// duration in minutes.
    ///
    /// # Examples
    ///
    /// ```
    /// use quantoxide::models::{Period, OhlcResolution};
    ///
    /// let period = Period::try_from(10).unwrap();
    ///
    /// // Duration is candles * resolution in minutes
    /// let duration = period.as_duration(OhlcResolution::FiveMinutes);
    /// assert_eq!(duration.num_minutes(), 50);
    /// ```
    pub fn as_duration(&self, resolution: OhlcResolution) -> Duration {
        Duration::minutes(self.0 as i64 * resolution.as_minutes() as i64)
    }

    /// Returns the number of candles as a `u64`.
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the number of candles as a `usize`.
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Returns the number of candles as an `i64`.
    pub const fn as_i64(&self) -> i64 {
        self.0 as i64
    }

    /// Returns the number of candles as an `f64`.
    pub const fn as_f64(&self) -> f64 {
        self.0 as f64
    }
}

impl TryFrom<u8> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u16> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: u16) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u32> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u64> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(PeriodValidationError::TooShort { value });
        }

        if value > Self::MAX.0 {
            return Err(PeriodValidationError::TooLong { value });
        }

        Ok(Self(value))
    }
}

impl TryFrom<i8> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: i8) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<i16> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: i16) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<i32> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: i32) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<i64> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: i64) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<usize> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: usize) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<isize> for Period {
    type Error = PeriodValidationError;

    fn try_from(value: isize) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl fmt::Display for Period {
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
            return Err(MinIterationIntervalValidationError::TooShort { value });
        }

        if value > Self::MAX.0 {
            return Err(MinIterationIntervalValidationError::TooLong { value });
        }

        Ok(Self(value))
    }
}

impl fmt::Display for MinIterationInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Historical candle data configuration specifying resolution and period.
///
/// Combines resolution (candle size) and period (number of candles) into a single configuration.
/// When an operator or evaluator needs historical candle data, both values are required together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lookback {
    resolution: OhlcResolution,
    period: Period,
}

impl Lookback {
    /// Creates a new lookback configuration with the specified resolution and period.
    pub fn new<P, E>(resolution: OhlcResolution, period: P) -> Result<Self, E>
    where
        P: TryInto<Period, Error = E>,
    {
        Ok(Self {
            resolution,
            period: period.try_into()?,
        })
    }

    /// Returns the candle resolution.
    pub fn resolution(&self) -> OhlcResolution {
        self.resolution
    }

    /// Returns the lookback period (number of candles).
    pub fn period(&self) -> Period {
        self.period
    }

    /// Returns the lookback as a duration.
    pub fn as_duration(&self) -> Duration {
        self.period.as_duration(self.resolution)
    }
}

impl Default for Lookback {
    fn default() -> Self {
        Self {
            resolution: OhlcResolution::FiveMinutes,
            period: Period(20),
        }
    }
}

impl fmt::Display for Lookback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} x {}", self.period, self.resolution)
    }
}
