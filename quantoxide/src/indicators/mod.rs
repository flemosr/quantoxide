use std::{collections::VecDeque, num::NonZeroU64};

use chrono::{DateTime, Duration, TimeDelta, Utc};

use crate::{
    db::models::{PartialPriceHistoryEntryLOCF, PriceHistoryEntryLOCF},
    util::DateTimeExt,
};

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

    /// Calculates the timestamp of the first LOCF entry needed to compute indicators for
    /// the given timestamp.
    ///
    /// # Arguments
    ///
    /// * `locf_sec` - A valid LOCF timestamp
    ///
    /// # Returns
    ///
    /// Returns the timestamp corresponding to the first entry of the LOCF range that is
    /// necessary to calculate the indicators corresponding to `locf_sec`.
    /// This timestamp is `WINDOW_SIZE_SEC - 1` seconds earlier than the input to account
    /// for the rolling window requirements of indicator calculations.
    pub fn get_first_required_locf_entry(locf_sec: DateTime<Utc>) -> DateTime<Utc> {
        locf_sec - Self::WINDOW_DIFF
    }

    /// Calculates the timestamp of the last LOCF entry whose indicators will be
    /// affected by the LOCF entry corresponding to the given timestamp.
    ///
    /// # Arguments
    ///
    /// * `locf_sec` - A valid LOCF timestamp
    ///
    /// # Returns
    ///
    /// Returns the timestamp corresponding to the last entry of the LOCF range whose
    /// indicators will be affected by the entry corresponding to `locf_sec`.
    /// This timestamp is `WINDOW_SIZE_SEC - 1` seconds later than the input to account
    /// for the rolling window requirements of indicator calculations.
    pub fn get_last_affected_locf_entry(locf_sec: DateTime<Utc>) -> DateTime<Utc> {
        locf_sec + Self::WINDOW_DIFF
    }

    pub fn evaluate(
        partial_locf_entries: Vec<PartialPriceHistoryEntryLOCF>,
        start_locf_sec: DateTime<Utc>,
    ) -> Result<Vec<PriceHistoryEntryLOCF>> {
        if partial_locf_entries.is_empty() {
            return Err(IndicatorError::EmptyInput);
        }

        let first_entry = partial_locf_entries.first().expect("not empty");
        if first_entry.time > start_locf_sec {
            return Err(IndicatorError::InvalidStartTime {
                first_entry_time: first_entry.time,
                start_time: start_locf_sec,
            });
        }

        let last_entry = partial_locf_entries.last().expect("not empty");
        if last_entry.time < start_locf_sec {
            return Err(IndicatorError::InvalidEndTime {
                last_entry_time: last_entry.time,
                start_time: start_locf_sec,
            });
        }

        let mut ma_5_eval = MovingAverageEvaluator::new(NonZeroU64::new(5).unwrap());
        let mut ma_60_eval = MovingAverageEvaluator::new(NonZeroU64::new(60).unwrap());
        let mut ma_300_eval = MovingAverageEvaluator::new(NonZeroU64::new(300).unwrap());

        let mut full_locf_entries = Vec::new();
        let mut last_time = None;

        for partial_entry in partial_locf_entries {
            if !partial_entry.time.is_round() {
                return Err(IndicatorError::InvalidEntryTime {
                    time: partial_entry.time,
                });
            }
            if let Some(last) = last_time {
                if partial_entry.time != last + Duration::seconds(1) {
                    return Err(IndicatorError::DiscontinuousEntries {
                        from: last,
                        to: partial_entry.time,
                    });
                }
            }

            last_time = Some(partial_entry.time);

            let ma_5 = ma_5_eval.update(partial_entry.value);
            let ma_60 = ma_60_eval.update(partial_entry.value);
            let ma_300 = ma_300_eval.update(partial_entry.value);

            if partial_entry.time >= start_locf_sec {
                full_locf_entries.push(PriceHistoryEntryLOCF {
                    time: partial_entry.time,
                    value: partial_entry.value,
                    ma_5,
                    ma_60,
                    ma_300,
                })
            }
        }

        Ok(full_locf_entries)
    }
}

#[cfg(test)]
mod test;
