use std::{pin::Pin, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use tokio::{sync::mpsc, time};

use lnm_sdk::api_v3::{RestClient, models::FundingSettlement};

use crate::{db::Database, util::DateTimeExt};

use super::super::config::{SyncFundingSettlementsTaskConfig, SyncProcessConfig};

pub(crate) mod error;
pub(in crate::sync) mod funding_settlements_state;

use error::{Result, SyncFundingSettlementsError};
use funding_settlements_state::{FundingDownloadRange, FundingSettlementsState};

// LN Markets funding settlement grid phases:
//
// Phase A: daily at 08 UTC (24h interval)
//   2021-01-11 08:00 .. 2021-12-07 08:00
//
// Phase B: 3x daily at 04, 12, 20 UTC (8h interval)
//   2021-12-07 20:00 .. 2025-04-11 04:00
//
// Phase C: 3x daily at 00, 08, 16 UTC (8h interval)
//   2025-04-11 16:00 .. present
//
// Dead zones (no settlements):
//   2021-12-07 08:00 .. 2021-12-07 20:00  (A→B transition, 12h gap)
//   2025-04-11 04:00 .. 2025-04-11 16:00  (B→C transition, 12h gap)

/// LN Markets phase A funding settlement interval (24 hours).
pub const LNM_SETTLEMENT_INTERVAL_DAY: Duration = Duration::hours(24);

/// LN Markets phase B/C funding settlement interval (8 hours).
pub const LNM_SETTLEMENT_INTERVAL_8H: Duration = Duration::hours(8);

/// First LN Markets funding settlement available from the API (phase A start).
pub const LNM_SETTLEMENT_A_START: DateTime<Utc> = NaiveDate::from_ymd_opt(2021, 1, 11)
    .expect("is valid")
    .and_hms_opt(8, 0, 0)
    .expect("is valid")
    .and_utc();

/// Last LN Markets funding settlement on the phase A grid ({08} UTC, daily).
pub const LNM_SETTLEMENT_A_END: DateTime<Utc> = NaiveDate::from_ymd_opt(2021, 12, 7)
    .expect("is valid")
    .and_hms_opt(8, 0, 0)
    .expect("is valid")
    .and_utc();

/// First LN Markets funding settlement on the phase B grid ({04, 12, 20} UTC).
pub const LNM_SETTLEMENT_B_START: DateTime<Utc> = NaiveDate::from_ymd_opt(2021, 12, 7)
    .expect("is valid")
    .and_hms_opt(20, 0, 0)
    .expect("is valid")
    .and_utc();

/// Last LN Markets funding settlement on the phase B grid ({04, 12, 20} UTC).
pub const LNM_SETTLEMENT_B_END: DateTime<Utc> = NaiveDate::from_ymd_opt(2025, 4, 11)
    .expect("is valid")
    .and_hms_opt(4, 0, 0)
    .expect("is valid")
    .and_utc();

/// First LN Markets funding settlement on the phase C grid ({00, 08, 16} UTC).
pub const LNM_SETTLEMENT_C_START: DateTime<Utc> = NaiveDate::from_ymd_opt(2025, 4, 11)
    .expect("is valid")
    .and_hms_opt(16, 0, 0)
    .expect("is valid")
    .and_utc();

pub(super) type FundingSettlementsStateTransmitter = mpsc::Sender<FundingSettlementsState>;

#[derive(Clone)]
pub(super) struct SyncFundingSettlementsTask {
    config: SyncFundingSettlementsTaskConfig,
    db: Arc<Database>,
    api_rest: Arc<RestClient>,
    funding_state_tx: Option<FundingSettlementsStateTransmitter>,
}

impl SyncFundingSettlementsTask {
    /// Returns a pinned sleep future that fires at the next phase C funding settlement time
    /// (00:00, 08:00, or 16:00 UTC).
    pub fn next_funding_timer() -> Pin<Box<time::Sleep>> {
        let now = Utc::now();
        assert!(
            now >= LNM_SETTLEMENT_C_START,
            "next_funding_timer requires phase C (now={now}, phase C start={LNM_SETTLEMENT_C_START})"
        );
        let next_time = now.ceil_funding_settlement_time();

        // If already exactly on-grid, advance one interval.
        let next_time = if next_time == now {
            now + LNM_SETTLEMENT_INTERVAL_8H
        } else {
            next_time
        };

        let wait = (next_time - now).to_std().unwrap_or(time::Duration::ZERO);
        Box::pin(time::sleep(wait))
    }

    pub fn new(
        config: &SyncProcessConfig,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        funding_state_tx: Option<FundingSettlementsStateTransmitter>,
    ) -> Self {
        Self {
            config: config.into(),
            db,
            api_rest,
            funding_state_tx,
        }
    }

    /// Fetches a single page of funding settlements from the API for the given download range.
    async fn get_new_settlements(
        &self,
        download_range: FundingDownloadRange,
    ) -> Result<Vec<FundingSettlement>> {
        let from = download_range.from();
        // The API's `to` parameter is inclusive, but it returns 0 entries when `from == to`.
        // Offset `to` by one second so single-settlement requests (e.g. Missing { from: T, to: T })
        // don't hit that edge case.
        let to = download_range.to().map(|t| t + Duration::seconds(1));

        time::sleep(self.config.rest_api_cooldown()).await;

        let mut trials = 0;
        let page = loop {
            match self
                .api_rest
                .futures_data
                .get_funding_settlements(from, to, None, None)
                .await
            {
                Ok(page) => break page,
                Err(error) => {
                    trials += 1;
                    if trials >= self.config.rest_api_error_max_trials().get() {
                        return Err(SyncFundingSettlementsError::RestApiMaxTrialsReached {
                            error,
                            trials: self.config.rest_api_error_max_trials(),
                        });
                    }

                    time::sleep(self.config.rest_api_error_cooldown()).await;
                    continue;
                }
            }
        };

        Ok(page.into())
    }

    /// Fetches a single page of settlements and inserts them into the DB.
    async fn partial_download(&self, download_range: FundingDownloadRange) -> Result<()> {
        let new_settlements = self.get_new_settlements(download_range).await?;

        for settlement in &new_settlements {
            if !settlement.time().is_valid_funding_settlement_time() {
                return Err(SyncFundingSettlementsError::InvalidSettlementTime {
                    time: settlement.time(),
                });
            }
        }

        if new_settlements.is_empty() {
            match download_range {
                FundingDownloadRange::LowerBound { to } => {
                    return Err(
                        SyncFundingSettlementsError::ApiSettlementsNotAvailableBeforeHistoryStart {
                            history_start: to,
                        },
                    );
                }
                FundingDownloadRange::Missing { .. }
                | FundingDownloadRange::Latest
                | FundingDownloadRange::UpperBound { .. } => {
                    // The caller (backfill) detects persistent gaps and advances past them.
                }
            }
        }

        self.db
            .funding_settlements
            .add_settlements(&new_settlements)
            .await?;

        Ok(())
    }

    async fn handle_state_update(&self, new_state: &FundingSettlementsState) -> Result<()> {
        if let Some(state_tx) = self.funding_state_tx.as_ref() {
            state_tx
                .send(new_state.clone())
                .await
                .map_err(|_| SyncFundingSettlementsError::HistoryUpdateHandlerFailed)?;
        }

        Ok(())
    }

    /// Runs the backfill process. Returns `true` if synced
    pub async fn backfill(self, flag_missing_range: Option<Duration>) -> Result<bool> {
        let mut exclude_missing_after: Option<DateTime<Utc>> = None;

        let mut state = FundingSettlementsState::evaluate_with_reach(
            &self.db,
            self.config.funding_settlement_reach(),
            flag_missing_range,
            exclude_missing_after,
        )
        .await?;
        self.handle_state_update(&state).await?;

        loop {
            let download_range = state.next_download_range(true)?;

            self.partial_download(download_range).await?;

            let new_state = FundingSettlementsState::evaluate_with_reach(
                &self.db,
                self.config.funding_settlement_reach(),
                flag_missing_range,
                exclude_missing_after,
            )
            .await?;
            self.handle_state_update(&new_state).await?;

            // The API may partially fill a missing group, but the latest entry (`to`) should always
            // be returned. If it persists, exclude it (one entry at a time) from the next
            // missing-range scans by shrinking `scan_to`, so subsequent cycles discover the next
            // missing entries further back, while not getting stuck with unfillable entries.
            match download_range {
                FundingDownloadRange::Missing { to, .. } if new_state.missing().contains(&to) => {
                    exclude_missing_after = download_range.to();
                    state = FundingSettlementsState::evaluate_with_reach(
                        &self.db,
                        self.config.funding_settlement_reach(),
                        flag_missing_range,
                        exclude_missing_after,
                    )
                    .await?;
                    self.handle_state_update(&state).await?;
                }
                _ => state = new_state,
            }

            if state.has_missing()? {
                continue;
            }

            let synced = state
                .bound_end()
                .is_some_and(|end| end >= Utc::now().floor_funding_settlement_time());

            if synced || download_range.to().is_none() {
                return Ok(synced);
            }
        }
    }
}
