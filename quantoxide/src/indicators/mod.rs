use std::{collections::VecDeque, num::NonZeroU64};

use chrono::{DateTime, Duration, Utc};

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
    pub const WINDOW_SIZE_SEC: usize = 300;

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
