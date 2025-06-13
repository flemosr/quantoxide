use std::fmt;

use chrono::{DateTime, Duration, Utc};

use crate::db::DbContext;

use super::error::{Result, SyncPriceHistoryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceHistoryState {
    reach_time_opt: Option<DateTime<Utc>>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    entry_gaps: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl PriceHistoryState {
    pub async fn evaluate(db: &DbContext, reach_opt: Option<Duration>) -> Result<Self> {
        let reach_time_opt = reach_opt.map_or(None, |reach| Some(Utc::now() - reach));

        let earliest_entry = match db.price_history.get_earliest_entry().await? {
            Some(entry) => entry,
            None => {
                // DB is empty

                return Ok(Self {
                    reach_time_opt,
                    bounds: None,
                    entry_gaps: Vec::new(),
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

            if reach_time_opt.map_or(false, |reach_time| earliest_entry.time < reach_time) {
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: earliest_entry.time,
                    reach: reach_time_opt.expect("`reach_time_opt` can't be `None`"),
                });
            }

            return Ok(Self {
                reach_time_opt,
                bounds: Some((earliest_entry.time, earliest_entry.time)),
                entry_gaps: Vec::new(),
            });
        }

        let entry_gaps = db.price_history.get_gaps().await?;

        if let Some((from_time, _)) = entry_gaps.first() {
            if reach_time_opt.map_or(false, |reach_time| *from_time < reach_time) {
                // There is a price gap before `reach_time`. Since entries before `reach_time`
                // can't be fetched, said gap can't be closed.
                // Therefore the DB can't be synced.
                return Err(SyncPriceHistoryError::UnreachableDbGap {
                    gap: *from_time,
                    reach: reach_time_opt.expect("`reach_time_opt` can't be `None`"),
                });
            }
        }

        Ok(Self {
            reach_time_opt,
            bounds: Some((earliest_entry.time, lastest_entry.time)),
            entry_gaps,
        })
    }

    pub fn next_download_range(
        &self,
        backfilling: bool,
    ) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
        let Some(reach_time) = self.reach_time_opt else {
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
            self.entry_gaps.first()
        } else {
            self.entry_gaps.last()
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

    pub fn get_upper_history_bound(&self) -> Option<DateTime<Utc>> {
        Some(self.bounds?.1)
    }

    pub fn tail_continuous_duration(&self) -> Option<Duration> {
        let history_bounds = &self.bounds?;

        if let Some((_, gap_to)) = self.entry_gaps.last() {
            return Some(history_bounds.1 - *gap_to);
        }

        Some(history_bounds.1 - history_bounds.0)
    }

    pub fn has_gaps(&self) -> Result<bool> {
        let Some(reach_time) = self.reach_time_opt else {
            return Err(SyncPriceHistoryError::Generic(
                "`reach` was not set".to_string(),
            ));
        };

        Ok(self.bounds.map_or(true, |bounds| {
            !self.entry_gaps.is_empty() || reach_time < bounds.0
        }))
    }
}

fn eval_missing_hours(current: &DateTime<Utc>, target: &DateTime<Utc>) -> String {
    let missing_hours = ((*current - *target).num_minutes() as f32 / 60. * 100.0).round() / 100.0;
    if missing_hours <= 0. {
        "Ok".to_string()
    } else {
        format!("missing {:.2} hours", missing_hours)
    }
}

impl fmt::Display for PriceHistoryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "PriceHistoryState:")?;
        if let Some(reach_time) = self.reach_time_opt {
            writeln!(f, "  reach: {}", reach_time.to_rfc3339())?;
        }

        match &self.bounds {
            Some((start, end)) => {
                writeln!(f, "  bounds:")?;

                if let Some(reach_time) = self.reach_time_opt {
                    let start_eval = eval_missing_hours(start, &reach_time);
                    writeln!(f, "    start: {} ({start_eval})", start.to_rfc3339())?;
                } else {
                    writeln!(f, "    start: {}", start.to_rfc3339())?;
                };

                let end_val = eval_missing_hours(&Utc::now(), end);
                writeln!(f, "    end: {} ({end_val})", end.to_rfc3339())?;

                if self.entry_gaps.is_empty() {
                    write!(f, "  gaps: no gaps")?;
                } else {
                    writeln!(f, "  gaps:")?;
                    for (i, (gap_start, gap_end)) in self.entry_gaps.iter().enumerate() {
                        let gap_hours = (*gap_end - *gap_start).num_minutes() as f32 / 60.;
                        writeln!(f, "    - gap {} (missing {:.2} hours):", i + 1, gap_hours)?;
                        writeln!(f, "        from: {}", gap_start.to_rfc3339())?;
                        if i == self.entry_gaps.len() - 1 {
                            write!(f, "        to: {}", gap_end.to_rfc3339())?;
                        } else {
                            writeln!(f, "        to: {}", gap_end.to_rfc3339())?;
                        }
                    }
                }
            }
            None => write!(f, "  bounds: database is empty")?,
        }

        Ok(())
    }
}
