use chrono::{DateTime, Utc};
use std::fmt;

use crate::{
    db::DbContext,
    error::{AppError, Result},
};

#[derive(Debug, Clone)]
pub struct PriceHistoryState {
    reach: DateTime<Utc>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    entry_gaps: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl PriceHistoryState {
    pub async fn evaluate(db: &DbContext, reach: DateTime<Utc>) -> Result<Self> {
        let earliest_entry = match db.price_history.get_earliest_entry().await? {
            Some(entry) => entry,
            None => {
                // DB is empty

                return Ok(Self {
                    reach,
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

            if earliest_entry.time < reach {
                return Err(AppError::UnreachableDbGap {
                    gap: earliest_entry.time,
                    reach,
                });
            }

            return Ok(Self {
                reach,
                bounds: Some((earliest_entry.time, earliest_entry.time)),
                entry_gaps: Vec::new(),
            });
        }

        let entry_gaps = db.price_history.get_gaps().await?;

        if let Some((from_time, _)) = entry_gaps.first() {
            if *from_time < reach {
                // There is a price gap before `reach`. Since we shouldn't fetch entries
                // before `reach`. Said gap can't be closed, and therefore the DB can't
                // be synced.
                return Err(AppError::UnreachableDbGap {
                    gap: *from_time,
                    reach,
                });
            }
        }

        Ok(Self {
            reach,
            bounds: Some((earliest_entry.time, lastest_entry.time)),
            entry_gaps,
        })
    }

    pub fn next_download_bounds(&self) -> (Option<&DateTime<Utc>>, Option<&DateTime<Utc>>) {
        let history_bounds = match &self.bounds {
            Some(bounds) => bounds,
            None => return (None, None),
        };

        if let Some((gap_from, gap_to)) = self.entry_gaps.first() {
            return (Some(gap_from), Some(gap_to));
        }
        if history_bounds.0 > self.reach {
            return (None, Some(&history_bounds.0));
        }
        (Some(&history_bounds.1), None)
    }

    pub fn has_gaps(&self) -> bool {
        self.bounds.is_none()
            || self.reach < self.bounds.expect("not none").0
            || !self.entry_gaps.is_empty()
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
        writeln!(f, "  reach: {}", self.reach.to_rfc3339())?;

        match &self.bounds {
            Some((start, end)) => {
                let start_eval = eval_missing_hours(start, &self.reach);
                let end_val = eval_missing_hours(&Utc::now(), end);

                writeln!(f, "  bounds:")?;
                writeln!(f, "    start: {} ({start_eval})", start.to_rfc3339())?;
                writeln!(f, "    end: {} ({end_val})", end.to_rfc3339())?;

                // Only show gaps section if database is not empty
                if self.entry_gaps.is_empty() {
                    writeln!(f, "  gaps: no gaps")?;
                } else {
                    writeln!(f, "  gaps:")?;
                    for (i, (gap_start, gap_end)) in self.entry_gaps.iter().enumerate() {
                        let gap_hours = (*gap_end - *gap_start).num_minutes() as f32 / 60.;
                        writeln!(f, "    - gap {} (missing {:.2} hours):", i + 1, gap_hours)?;
                        writeln!(f, "        from: {}", gap_start.to_rfc3339())?;
                        writeln!(f, "        to: {}", gap_end.to_rfc3339())?;
                    }
                }
            }
            None => writeln!(f, "  bounds: database is empty")?,
        }

        Ok(())
    }
}
