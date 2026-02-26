use std::fmt;

use chrono::{DateTime, Duration, Utc};

use crate::db::Database;

use crate::util::DateTimeExt;

use super::{
    LNM_SETTLEMENT_INTERVAL_8H,
    error::{Result, SyncFundingSettlementsError},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum FundingDownloadRange {
    Latest,
    UpperBound {
        from: DateTime<Utc>,
    },
    Missing {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    LowerBound {
        to: DateTime<Utc>,
    },
}

impl FundingDownloadRange {
    pub fn from(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::UpperBound { from } | Self::Missing { from, to: _ } => Some(*from),
            _ => None,
        }
    }

    pub fn to(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::LowerBound { to } | Self::Missing { from: _, to } => Some(*to),
            _ => None,
        }
    }
}

/// Represents the current state of funding settlement data in the database.
///
/// Tracks the time range of available funding settlement data, identifies missing settlement
/// times on the settlement grid, and determines what additional data needs to be fetched to achieve
/// complete synchronization within a specified reach period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingSettlementsState {
    reach_time: Option<DateTime<Utc>>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    missing: Vec<DateTime<Utc>>,
}

impl FundingSettlementsState {
    async fn new(
        db: &Database,
        reach_time: Option<DateTime<Utc>>,
        flag_missing_range: Option<Duration>,
        exclude_missing_after: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let Some(earliest_time) = db
            .funding_settlements
            .get_earliest_settlement_time()
            .await?
        else {
            return Ok(Self {
                reach_time,
                bounds: None,
                missing: Vec::new(),
            });
        };

        let latest_time = db
            .funding_settlements
            .get_latest_settlement_time()
            .await?
            .expect("db not empty");

        if earliest_time == latest_time {
            if reach_time.is_some_and(|reach_time| earliest_time < reach_time) {
                return Err(SyncFundingSettlementsError::UnreachableMissingSettlement {
                    time: earliest_time,
                    reach: reach_time.expect("`reach_time_opt` can't be `None`"),
                });
            }

            return Ok(Self {
                reach_time,
                bounds: Some((earliest_time, earliest_time)),
                missing: Vec::new(),
            });
        }

        let missing = match flag_missing_range {
            Some(range) => {
                let scan_from = (Utc::now() - range)
                    .max(earliest_time)
                    .ceil_funding_settlement_time();
                let scan_to = exclude_missing_after
                    .map(|t| (t - Duration::seconds(1)).floor_funding_settlement_time())
                    .unwrap_or(latest_time);
                if scan_from > scan_to {
                    Vec::new()
                } else {
                    db.funding_settlements
                        .get_missing_settlement_times(scan_from, scan_to)
                        .await?
                }
            }
            None => Vec::new(),
        };

        if let Some(first_missing) = missing.first()
            && reach_time.is_some_and(|reach_time| *first_missing < reach_time)
        {
            return Err(SyncFundingSettlementsError::UnreachableMissingSettlement {
                time: *first_missing,
                reach: reach_time.expect("`reach_time_opt` can't be `None`"),
            });
        }

        Ok(Self {
            reach_time,
            bounds: Some((earliest_time, latest_time)),
            missing,
        })
    }

    /// Evaluates the current funding settlements state from the database, without scanning for
    /// missing settlements.
    pub async fn evaluate(db: &Database) -> Result<Self> {
        Self::new(db, None, None, None).await
    }

    /// Evaluates the current funding settlements state from the database, scanning for missing
    /// settlements within the given `flag_missing_range` (looking back from now), up to
    /// `exclude_missing_after` (exclusive upper bound).
    pub(crate) async fn evaluate_with_reach(
        db: &Database,
        reach: DateTime<Utc>,
        flag_missing_range: Option<Duration>,
        exclude_missing_after: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        Self::new(db, Some(reach), flag_missing_range, exclude_missing_after).await
    }

    /// Returns the time bounds of the available funding settlement data.
    pub fn bounds(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.bounds
    }

    /// Returns the list of missing settlement times on the funding settlement grid.
    pub fn missing(&self) -> &Vec<DateTime<Utc>> {
        &self.missing
    }

    /// Returns the start time of the funding settlement bounds.
    pub fn bound_start(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(start, _)| start)
    }

    /// Returns the end time of the funding settlement bounds.
    pub fn bound_end(&self) -> Option<DateTime<Utc>> {
        self.bounds.map(|(_, end)| end)
    }

    /// Checks if the specified time range falls within the available settlement bounds.
    pub fn is_range_available(&self, range_from: DateTime<Utc>, range_to: DateTime<Utc>) -> bool {
        self.bounds
            .is_some_and(|(start, end)| start <= range_from && end >= range_to)
    }

    /// Returns the latest contiguous group of missing settlement times.
    ///
    /// Always prioritizes the most recent missing settlements, working backwards in time.
    /// Uses the 8h interval as the gap threshold, so Phase A missing entries (24h apart) each form
    /// their own single-element group — this is fine since the API page covers individual
    /// timestamps.
    fn latest_missing_group(&self) -> Option<&[DateTime<Utc>]> {
        if self.missing.is_empty() {
            return None;
        }

        let start = self
            .missing
            .windows(2)
            .rposition(|w| w[1] - w[0] > LNM_SETTLEMENT_INTERVAL_8H)
            .map_or(0, |i| i + 1);

        Some(&self.missing[start..])
    }

    /// When `backfilling` is:
    /// + `true` -> extends history backwards toward `reach_time`
    /// + `false` -> live mode, only extends forward
    pub(crate) fn next_download_range(&self, backfilling: bool) -> Result<FundingDownloadRange> {
        let bounds = match &self.bounds {
            Some(bounds) => bounds,
            None => return Ok(FundingDownloadRange::Latest),
        };

        if self
            .reach_time
            .is_some_and(|reach_time| bounds.0 == bounds.1 && bounds.0 < reach_time)
        {
            return Err(SyncFundingSettlementsError::UnreachableMissingSettlement {
                time: bounds.0,
                reach: self.reach_time.expect("not `None`"),
            });
        }

        // Always start from the most recent missing settlements and work backwards
        if let Some(group) = self.latest_missing_group() {
            let first = *group.first().expect("group is non-empty");
            let last = *group.last().expect("group is non-empty");

            if self.reach_time.is_some_and(|reach_time| first < reach_time) {
                return Err(SyncFundingSettlementsError::UnreachableMissingSettlement {
                    time: first,
                    reach: self.reach_time.expect("not `None`"),
                });
            }

            return Ok(FundingDownloadRange::Missing {
                from: first,
                to: last,
            });
        }

        if self
            .reach_time
            .is_some_and(|reach_time| backfilling && bounds.0 > reach_time)
        {
            return Ok(FundingDownloadRange::LowerBound { to: bounds.0 });
        }

        Ok(FundingDownloadRange::UpperBound { from: bounds.1 })
    }

    /// Checks whether there are any missing settlements within the reach period (DB is empty,
    /// interior gaps exist, or history doesn't reach back far enough).
    pub fn has_missing(&self) -> Result<bool> {
        let Some(reach_time) = self.reach_time else {
            return Err(SyncFundingSettlementsError::FundingSettlementsStateReachNotSet);
        };

        Ok(self
            .bounds
            .is_none_or(|bounds| !self.missing.is_empty() || reach_time < bounds.0))
    }

    fn eval_missing_hours(current: &DateTime<Utc>, target: &DateTime<Utc>) -> String {
        let previous_settlement_time = current.floor_funding_settlement_time();
        let missing_hours =
            ((previous_settlement_time - *target).num_minutes() as f32 / 60. * 100.0).round()
                / 100.0;
        if missing_hours <= 0. {
            "Ok".to_string()
        } else {
            format!("missing {:.2} hours", missing_hours)
        }
    }

    /// Generates a human-readable summary of the funding settlements state.
    pub fn summary(&self) -> String {
        let mut result = String::new();

        if let Some(reach_time) = self.reach_time {
            result.push_str(&format!(
                "reach: {}\n",
                reach_time.format("%Y-%m-%d %H:%M %Z")
            ));
        }

        match &self.bounds {
            Some((start, end)) => {
                result.push_str("bounds:\n");

                if let Some(reach_time) = self.reach_time {
                    let start_eval = Self::eval_missing_hours(start, &reach_time);
                    let start_str = start.format("%Y-%m-%d %H:%M %Z");
                    result.push_str(&format!("  start: {start_str} ({start_eval})\n"));
                } else {
                    let start_str = start.format("%Y-%m-%d %H:%M %Z");
                    result.push_str(&format!("  start: {start_str}\n"));
                };

                let end_val = Self::eval_missing_hours(&Utc::now(), end);
                let end_str = end.format("%Y-%m-%d %H:%M %Z");
                result.push_str(&format!("  end: {end_str} ({end_val})\n"));

                if self.missing.is_empty() {
                    result.push_str("missing: none");
                } else {
                    result.push_str(&format!("missing: {} settlement(s)\n", self.missing.len()));
                    for (i, time) in self.missing.iter().enumerate() {
                        let time_str = time.format("%Y-%m-%d %H:%M %Z");
                        if i == self.missing.len() - 1 {
                            result.push_str(&format!("  - {time_str}"));
                        } else {
                            result.push_str(&format!("  - {time_str}\n"));
                        }
                    }
                }
            }
            None => result.push_str("bounds: database is empty"),
        }

        result
    }
}

impl fmt::Display for FundingSettlementsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Funding Settlements State:")?;
        for line in self.summary().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
