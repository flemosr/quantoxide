use std::{collections::VecDeque, num::NonZeroU64};

use chrono::{DateTime, Duration, TimeDelta, Utc};

use crate::{db::models::PartialPriceHistoryEntryLOCF, util::DateTimeExt};

pub mod error;

use error::{IndicatorError, Result};

#[derive(Debug)]
pub struct IndicatorValues {
    time: DateTime<Utc>,
    ma_5: Option<f64>,
    ma_60: Option<f64>,
    ma_300: Option<f64>,
}

impl IndicatorValues {
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn ma_5(&self) -> Option<f64> {
        self.ma_5
    }

    pub fn ma_60(&self) -> Option<f64> {
        self.ma_60
    }

    pub fn ma_300(&self) -> Option<f64> {
        self.ma_300
    }
}

struct MovingAverageEvaluator {
    window: VecDeque<f64>,
    sum: f64,
    period: usize,
}

impl MovingAverageEvaluator {
    fn new(period: NonZeroU64) -> Self {
        Self {
            window: VecDeque::new(),
            sum: 0.,
            period: period.get() as usize,
        }
    }

    fn update(&mut self, value: f64) -> Option<f64> {
        self.sum += value;
        self.window.push_back(value);

        if self.window.len() > self.period {
            let removed = self.window.pop_front().expect("window can't be empty");
            self.sum -= removed;
        }

        if self.window.len() == self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
    }
}

pub struct IndicatorsEvaluator;

impl IndicatorsEvaluator {
    const WINDOW_SIZE_SEC: usize = 300; // From MovingAverage 300

    const WINDOW_DIFF: TimeDelta = Duration::seconds(Self::WINDOW_SIZE_SEC as i64 - 1);

    /// Calculates the data range required to evaluate indicators affected by updated LOCF entries.
    ///
    /// # Arguments
    ///
    /// * `start_locf_sec` - The start timestamp of the updated LOCF entries range
    /// * `end_locf_sec` - The end timestamp of the updated LOCF entries range
    ///
    /// # Returns
    ///
    /// Returns a tuple containing:
    /// * `start_indicator_sec` - The earliest timestamp needed for indicator data fetching
    /// * `end_indicator_sec` - The latest timestamp needed for indicator data fetching
    ///
    /// The returned range is expanded by `WINDOW_SIZE_SEC - 1` seconds on both sides to account
    /// for the rolling window requirements of indicator calculations.
    ///
    /// # Errors
    ///
    /// Returns `IndicatorError::Generic` if `end_locf_sec` is earlier than `start_locf_sec`.
    pub fn get_indicator_calculation_range(
        start_locf_sec: DateTime<Utc>,
        end_locf_sec: DateTime<Utc>,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
        if end_locf_sec < start_locf_sec {
            return Err(IndicatorError::Generic(format!(
                "end_locf_sec lt start_locf_sec"
            )));
        }

        let start_indicator_sec = start_locf_sec - Self::WINDOW_DIFF;
        let end_indicator_sec = end_locf_sec + Self::WINDOW_DIFF;

        Ok((start_indicator_sec, end_indicator_sec))
    }

    pub fn evaluate(
        locf_entries: Vec<PartialPriceHistoryEntryLOCF>,
        start_locf_sec: DateTime<Utc>,
    ) -> Result<Vec<IndicatorValues>> {
        if locf_entries.is_empty() {
            return Err(IndicatorError::Generic(format!("locf_entries is empty")));
        }
        if locf_entries.first().expect("not empty").time > start_locf_sec {
            return Err(IndicatorError::Generic(format!(
                "initial locf entries time gt than start_locf_sec"
            )));
        }
        if locf_entries.last().expect("not empty").time < start_locf_sec {
            return Err(IndicatorError::Generic(format!(
                "closing locf entries time lt than start_locf_sec"
            )));
        }

        let mut ma_5_eval = MovingAverageEvaluator::new(NonZeroU64::new(5).unwrap());
        let mut ma_60_eval = MovingAverageEvaluator::new(NonZeroU64::new(60).unwrap());
        let mut ma_300_eval = MovingAverageEvaluator::new(NonZeroU64::new(300).unwrap());

        let mut indicators = Vec::new();
        let mut last_time = None;

        for entry in locf_entries {
            if !entry.time.is_round() {
                return Err(IndicatorError::Generic(format!(
                    "locf entry with invalid time {}",
                    entry.time
                )));
            }
            if last_time.map_or(false, |last_time| {
                entry.time != last_time + Duration::seconds(1)
            }) {
                return Err(IndicatorError::Generic(format!(
                    "locf entries are not continuous. jumped from {} to {}",
                    last_time.expect("last_time can't be None"),
                    entry.time
                )));
            }

            last_time = Some(entry.time);

            let ma_5 = ma_5_eval.update(entry.value);
            let ma_60 = ma_60_eval.update(entry.value);
            let ma_300 = ma_300_eval.update(entry.value);

            if entry.time >= start_locf_sec {
                indicators.push(IndicatorValues {
                    time: entry.time,
                    ma_5,
                    ma_60,
                    ma_300,
                })
            }
        }

        Ok(indicators)
    }
}
