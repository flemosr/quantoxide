use std::fmt;

use chrono::{DateTime, Duration, Utc};

use crate::{db::Database, util::DateTimeExt};

use super::error::{Result, SyncPriceHistoryError};

#[derive(Debug, Clone, Copy)]
pub(crate) enum DownloadRange {
    Latest,
    UpperBound {
        from: DateTime<Utc>,
    },
    Gap {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    LowerBound {
        to: DateTime<Utc>,
    },
}

impl DownloadRange {
    pub fn from(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::UpperBound { from } | Self::Gap { from, to: _ } => Some(*from),
            _ => None,
        }
    }

    pub fn to(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::LowerBound { to } | Self::Gap { from: _, to } => Some(*to),
            _ => None,
        }
    }
}

/// Represents the current state of price history data in the database.
///
/// Tracks the time range of available price data, identifies gaps in the data, and determines what
/// additional data needs to be fetched to achieve complete synchronization within a specified reach
/// period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceHistoryState {
    reach_time: Option<DateTime<Utc>>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    gaps: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl PriceHistoryState {
    async fn new(db: &Database, reach_opt: Option<Duration>) -> Result<Self> {
        let reach_time = reach_opt.map(|reach| Utc::now() - reach);

        let Some(earliest_candle_time) = db.ohlc_candles.get_earliest_stable_candle_time().await?
        else {
            // DB is empty
            return Ok(Self {
                reach_time,
                bounds: None,
                gaps: Vec::new(),
            });
        };

        let lastest_candle_time = db
            .ohlc_candles
            .get_latest_stable_candle_time()
            .await?
            .expect("db not empty");

        if earliest_candle_time == lastest_candle_time {
            // DB has a single candle

            if reach_time.is_some_and(|reach_time| earliest_candle_time < reach_time) {
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: earliest_candle_time,
                    reach: reach_time.expect("`reach_time_opt` can't be `None`"),
                });
            }

            return Ok(Self {
                reach_time,
                bounds: Some((earliest_candle_time, earliest_candle_time)),
                gaps: Vec::new(),
            });
        }

        let gaps = db.ohlc_candles.get_gaps().await?;

        if let Some((from_time, _)) = gaps.first()
            && reach_time.is_some_and(|reach_time| *from_time < reach_time)
        {
            // There is a price gap before `reach_time`. Since candles before `reach_time`
            // can't be fetched, said gap can't be closed.
            // Therefore the DB can't be synced.
            return Err(SyncPriceHistoryError::UnreachableDbGap {
                gap: *from_time,
                reach: reach_time.expect("`reach_time_opt` can't be `None`"),
            });
        }

        Ok(Self {
            reach_time,
            bounds: Some((earliest_candle_time, lastest_candle_time)),
            gaps,
        })
    }

    /// Evaluates the current price history state from the database.
    ///
    /// Analyzes the database to determine data bounds and gaps without imposing a reach time limit.
    pub async fn evaluate(db: &Database) -> Result<Self> {
        Self::new(db, None).await
    }

    pub(crate) async fn evaluate_with_reach(db: &Database, reach: Duration) -> Result<Self> {
        Self::new(db, Some(reach)).await
    }

    /// Returns the time bounds of the available price history data.
    ///
    /// Returns `None` if the database is empty, otherwise returns a tuple of
    /// (`earliest_time`, `latest_time`).
    pub fn bounds(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.bounds
    }

    /// Returns the list of time gaps in the price history data.
    ///
    /// Each gap is represented as a tuple of (`from_time`, `to_time`).
    pub fn gaps(&self) -> &Vec<(DateTime<Utc>, DateTime<Utc>)> {
        &self.gaps
    }

    /// Returns the start time of the price history bounds.
    pub fn bound_start(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(start, _)| start)
    }

    /// Returns the end time of the price history bounds.
    pub fn bound_end(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(_, end)| end)
    }

    /// Checks if a specified time range is fully available without gaps.
    pub fn is_range_available(
        &self,
        range_from: DateTime<Utc>,
        range_to: DateTime<Utc>,
    ) -> Result<bool> {
        if range_from >= range_to {
            return Err(SyncPriceHistoryError::InvalidPriceHistoryStateRange {
                range_from,
                range_to,
            });
        }

        let Some(bounds) = self.bounds else {
            return Ok(false);
        };

        let range_within_bounds = bounds.0 <= range_from && bounds.1 >= range_to;
        let range_without_gaps = !self
            .gaps
            .iter()
            .any(|(gap_from, gap_to)| range_from < *gap_to && *gap_from < range_to);

        Ok(range_within_bounds && range_without_gaps)
    }

    pub(crate) fn next_download_range(&self, backfilling: bool) -> Result<DownloadRange> {
        let history_bounds = match &self.bounds {
            Some(bounds) => bounds,
            None => return Ok(DownloadRange::Latest),
        };

        if self.reach_time.is_some_and(|reach_time| {
            history_bounds.0 == history_bounds.1 && history_bounds.0 < reach_time
        }) {
            // DB has a single unreachable entry. Edge case

            return Err(SyncPriceHistoryError::UnreachableDbGap {
                gap: history_bounds.0,
                reach: self.reach_time.expect("not `None`"),
            });
        }

        let prioritized_gap = if backfilling {
            self.gaps.first()
        } else {
            self.gaps.last()
        };

        if let Some((from, to)) = prioritized_gap.cloned() {
            if self.reach_time.is_some_and(|reach_time| from < reach_time) {
                // Gap before `reach`. Since entries before `reach` can't be fetched, said gap
                // can't be closed. Therefore, the DB can't be synced.
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: from,
                    reach: self.reach_time.expect("not `None`"),
                });
            }

            return Ok(DownloadRange::Gap { from, to });
        }

        if self
            .reach_time
            .is_some_and(|reach_time| backfilling && history_bounds.0 > reach_time)
        {
            // Price history should be extended further into the past

            return Ok(DownloadRange::LowerBound {
                to: history_bounds.0,
            });
        }

        Ok(DownloadRange::UpperBound {
            from: history_bounds.1,
        })
    }

    /// Returns the duration of continuous (gap-free) price history at the tail end.
    ///
    /// This is the time span from the end of the most recent gap to the latest data point.
    /// If there are no gaps, returns the total duration of all available price history.
    pub fn tail_continuous_duration(&self) -> Option<Duration> {
        let history_bounds = &self.bounds?;

        if let Some((_, gap_to)) = self.gaps.last() {
            return Some(history_bounds.1 - *gap_to);
        }

        Some(history_bounds.1 - history_bounds.0)
    }

    /// Checks whether there are any gaps or missing data within the reach period.
    pub fn has_gaps(&self) -> Result<bool> {
        let Some(reach_time) = self.reach_time else {
            return Err(SyncPriceHistoryError::PriceHistoryStateReachNotSet);
        };

        Ok(self
            .bounds
            .is_none_or(|bounds| !self.gaps.is_empty() || reach_time < bounds.0))
    }

    fn eval_missing_hours(current: &DateTime<Utc>, target: &DateTime<Utc>) -> String {
        let missing_hours =
            ((*current - *target).num_minutes() as f32 / 60. * 100.0).round() / 100.0;
        if missing_hours <= 0. {
            "Ok".to_string()
        } else {
            format!("missing {:.2} hours", missing_hours)
        }
    }

    /// Generates a human-readable summary of the price history state.
    pub fn summary(&self) -> String {
        let mut result = String::new();

        if let Some(reach_time) = self.reach_time {
            result.push_str(&format!("reach: {}\n", reach_time.format_local_millis()));
        }

        match &self.bounds {
            Some((start, end)) => {
                result.push_str("bounds:\n");

                if let Some(reach_time) = self.reach_time {
                    let start_eval = Self::eval_missing_hours(start, &reach_time);
                    let start_str = start.format_local_millis();
                    result.push_str(&format!("  start: {start_str} ({start_eval})\n"));
                } else {
                    let start_str = start.format_local_millis();
                    result.push_str(&format!("  start: {start_str}\n"));
                };

                let end_val = Self::eval_missing_hours(&Utc::now(), end);
                let end_str = end.format_local_millis();
                result.push_str(&format!("  end: {end_str} ({end_val})\n"));

                if self.gaps.is_empty() {
                    result.push_str("gaps: no gaps\n");
                } else {
                    result.push_str("gaps:\n");
                    for (i, (gap_start, gap_end)) in self.gaps.iter().enumerate() {
                        let gap_hours = (*gap_end - *gap_start).num_minutes() as f32 / 60.;
                        result.push_str(&format!(
                            "  - gap {} (missing {:.2} hours):\n",
                            i + 1,
                            gap_hours
                        ));
                        let gap_start_str = gap_start.format_local_millis();
                        result.push_str(&format!("      from: {gap_start_str}\n"));

                        let gap_end_str = gap_end.format_local_millis();
                        if i == self.gaps.len() - 1 {
                            result.push_str(&format!("      to: {gap_end_str}"));
                        } else {
                            result.push_str(&format!("      to: {gap_end_str}\n"));
                        }
                    }
                }
            }
            None => result.push_str("bounds: database is empty"),
        }

        result
    }
}

impl fmt::Display for PriceHistoryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Price History State:")?;
        for line in self.summary().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
