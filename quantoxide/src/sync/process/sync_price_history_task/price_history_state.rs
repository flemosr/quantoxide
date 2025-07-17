use std::fmt;

use chrono::{DateTime, Duration, Utc};

use crate::db::DbContext;

use super::error::{Result, SyncPriceHistoryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceHistoryState {
    reach_time: Option<DateTime<Utc>>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    gaps: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl PriceHistoryState {
    async fn new(db: &DbContext, reach_opt: Option<Duration>) -> Result<Self> {
        let reach_time = reach_opt.map_or(None, |reach| Some(Utc::now() - reach));

        let earliest_entry = match db.price_history.get_earliest_entry().await? {
            Some(entry) => entry,
            None => {
                // DB is empty

                return Ok(Self {
                    reach_time,
                    bounds: None,
                    gaps: Vec::new(),
                });
            }
        };

        let lastest_entry = db
            .price_history
            .get_latest_entry()
            .await?
            .expect("db not empty");

        if earliest_entry.time == lastest_entry.time {
            // DB has a single entry

            if reach_time.map_or(false, |reach_time| earliest_entry.time < reach_time) {
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: earliest_entry.time,
                    reach: reach_time.expect("`reach_time_opt` can't be `None`"),
                });
            }

            return Ok(Self {
                reach_time,
                bounds: Some((earliest_entry.time, earliest_entry.time)),
                gaps: Vec::new(),
            });
        }

        let entry_gaps = db.price_history.get_gaps().await?;

        if let Some((from_time, _)) = entry_gaps.first() {
            if reach_time.map_or(false, |reach_time| *from_time < reach_time) {
                // There is a price gap before `reach_time`. Since entries before `reach_time`
                // can't be fetched, said gap can't be closed.
                // Therefore the DB can't be synced.
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: *from_time,
                    reach: reach_time.expect("`reach_time_opt` can't be `None`"),
                });
            }
        }

        Ok(Self {
            reach_time,
            bounds: Some((earliest_entry.time, lastest_entry.time)),
            gaps: entry_gaps,
        })
    }

    pub async fn evaluate(db: &DbContext) -> Result<Self> {
        Self::new(db, None).await
    }

    pub(crate) async fn evaluate_with_reach(db: &DbContext, reach: Duration) -> Result<Self> {
        Self::new(db, Some(reach)).await
    }

    pub fn bounds(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.bounds
    }
    pub fn gaps(&self) -> &Vec<(DateTime<Utc>, DateTime<Utc>)> {
        &self.gaps
    }

    pub fn bound_start(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(start, _)| start)
    }

    pub fn bound_end(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(_, end)| end)
    }

    pub fn is_range_available(
        &self,
        range_from: DateTime<Utc>,
        range_to: DateTime<Utc>,
    ) -> Result<bool> {
        if range_from >= range_to {
            return Err(SyncPriceHistoryError::Generic(
                "`from` gte `to`".to_string(),
            ));
        }

        let Some(bounds) = self.bounds else {
            return Err(SyncPriceHistoryError::Generic(
                "price history is empty".to_string(),
            ));
        };

        let range_within_bounds = bounds.0 <= range_from && bounds.1 >= range_to;
        let range_without_gaps = !self
            .gaps
            .iter()
            .any(|(gap_from, gap_to)| range_from < *gap_to && *gap_from < range_to);

        Ok(range_within_bounds && range_without_gaps)
    }

    pub(crate) fn next_download_range(
        &self,
        backfilling: bool,
    ) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
        let Some(reach_time) = self.reach_time else {
            return Err(SyncPriceHistoryError::Generic(
                "`reach` was not set".to_string(),
            ));
        };

        let history_bounds = match &self.bounds {
            Some(bounds) => bounds,
            None => return Ok((None, None)),
        };

        if history_bounds.0 == history_bounds.1 && history_bounds.0 < reach_time {
            // DB has a single unreachable entry. Edge case

            return Err(SyncPriceHistoryError::UnreachableDbGap {
                gap: history_bounds.0,
                reach: reach_time,
            });
        }

        let prioritized_gap = if backfilling {
            self.gaps.first()
        } else {
            self.gaps.last()
        };

        if let Some((gap_from, gap_to)) = prioritized_gap {
            if *gap_from < reach_time {
                // Gap before `reach`. Since entries before `reach` can't be fetched, said gap
                // can't be closed. Therefore, the DB can't be synced.
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: *gap_from,
                    reach: reach_time,
                });
            }

            return Ok((Some(*gap_from), Some(*gap_to)));
        }

        if backfilling && history_bounds.0 > reach_time {
            // Price history can be extended further into the past

            return Ok((None, Some(history_bounds.0)));
        }

        Ok((Some(history_bounds.1), None))
    }

    pub fn tail_continuous_duration(&self) -> Option<Duration> {
        let history_bounds = &self.bounds?;

        if let Some((_, gap_to)) = self.gaps.last() {
            return Some(history_bounds.1 - *gap_to);
        }

        Some(history_bounds.1 - history_bounds.0)
    }

    pub(crate) fn has_gaps(&self) -> Result<bool> {
        let Some(reach_time) = self.reach_time else {
            return Err(SyncPriceHistoryError::Generic(
                "`reach` was not set".to_string(),
            ));
        };

        Ok(self.bounds.map_or(true, |bounds| {
            !self.gaps.is_empty() || reach_time < bounds.0
        }))
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

    pub fn summary(&self) -> String {
        let mut result = String::new();

        if let Some(reach_time) = self.reach_time {
            result.push_str(&format!("reach: {}\n", reach_time.to_rfc3339()));
        }

        match &self.bounds {
            Some((start, end)) => {
                result.push_str("bounds:\n");

                if let Some(reach_time) = self.reach_time {
                    let start_eval = Self::eval_missing_hours(start, &reach_time);
                    result.push_str(&format!("  start: {} ({start_eval})\n", start.to_rfc3339()));
                } else {
                    result.push_str(&format!("  start: {}\n", start.to_rfc3339()));
                };

                let end_val = Self::eval_missing_hours(&Utc::now(), end);
                result.push_str(&format!("  end: {} ({end_val})\n", end.to_rfc3339()));

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
                        result.push_str(&format!("      from: {}\n", gap_start.to_rfc3339()));
                        if i == self.gaps.len() - 1 {
                            result.push_str(&format!("      to: {}", gap_end.to_rfc3339()));
                        } else {
                            result.push_str(&format!("      to: {}", gap_end.to_rfc3339()));
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
        write!(f, "PriceHistoryState:")?;
        for line in self.summary().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
