use std::result;

use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndicatorError {
    #[error("Invalid date range: end time ({end}) is before start time ({start})")]
    InvalidDateRange {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },

    #[error("Empty input: no LOCF entries provided")]
    EmptyInput,

    #[error(
        "Invalid start time: first entry time ({first_entry_time}) is after the requested start time ({start_time})"
    )]
    InvalidStartTime {
        first_entry_time: DateTime<Utc>,
        start_time: DateTime<Utc>,
    },

    #[error(
        "Invalid end time: last entry time ({last_entry_time}) is before the requested start time ({start_time})"
    )]
    InvalidEndTime {
        last_entry_time: DateTime<Utc>,
        start_time: DateTime<Utc>,
    },

    #[error("Invalid entry time: {time} is not aligned to second boundaries")]
    InvalidEntryTime { time: DateTime<Utc> },

    #[error(
        "Discontinuous entries: expected continuous second-by-second data, but jumped from {from} to {to}"
    )]
    DiscontinuousEntries {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
}

pub type Result<T> = result::Result<T, IndicatorError>;
