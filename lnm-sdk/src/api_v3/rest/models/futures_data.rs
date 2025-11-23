use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::shared::models::price::Price;

pub enum OhlcRange {
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
    OneWeek,
    OneMonth,
    ThreeMonths,
}

impl fmt::Display for OhlcRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            OhlcRange::OneMinute => "1m",
            OhlcRange::ThreeMinutes => "3m",
            OhlcRange::FiveMinutes => "5m",
            OhlcRange::TenMinutes => "10m",
            OhlcRange::FifteenMinutes => "15m",
            OhlcRange::ThirtyMinutes => "30m",
            OhlcRange::FortyFiveMinutes => "45m",
            OhlcRange::OneHour => "1h",
            OhlcRange::TwoHours => "2h",
            OhlcRange::ThreeHours => "3h",
            OhlcRange::FourHours => "4h",
            OhlcRange::OneDay => "1d",
            OhlcRange::OneWeek => "1w",
            OhlcRange::OneMonth => "1month",
            OhlcRange::ThreeMonths => "3months",
        };

        write!(f, "{}", s)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct OhlcCandle {
    time: DateTime<Utc>,
    open: Price,
    high: Price,
    low: Price,
    close: Price,
    volume: u64,
}

impl OhlcCandle {
    /// Timestamp of the OHLC candle.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Opening price.
    pub fn open(&self) -> Price {
        self.open
    }

    /// Highest price.
    pub fn high(&self) -> Price {
        self.high
    }

    /// Lowest price.
    pub fn low(&self) -> Price {
        self.low
    }

    /// Closing price.
    pub fn close(&self) -> Price {
        self.close
    }

    /// Trading volume.
    pub fn volume(&self) -> u64 {
        self.volume
    }
}
