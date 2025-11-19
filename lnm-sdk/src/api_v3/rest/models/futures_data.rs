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

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OhlcCandlePage {
    data: Vec<OhlcCandle>,
    next_cursor: Option<DateTime<Utc>>,
}

impl OhlcCandlePage {
    /// Vector of OHLC candles.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(ohlc_candles: lnm_sdk::api_v3::models::OhlcCandlePage) -> Result<(), Box<dyn std::error::Error>> {
    /// for ohlc_candle in ohlc_candles.data() {
    ///     println!("ohlc_candle: {:?}", ohlc_candle);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<OhlcCandle> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(ohlc_candles: lnm_sdk::api_v3::models::OhlcCandlePage) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = ohlc_candles.next_cursor() {
    ///     println!("More OHLC candles can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more OHLC candles available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}
