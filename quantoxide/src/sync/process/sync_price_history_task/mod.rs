use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use tokio::{sync::mpsc, time};

use lnm_sdk::{ApiClient, models::PriceEntryLNM};

use crate::db::DbContext;

use super::super::config::{SyncPriceHistoryTaskConfig, SyncProcessConfig};

mod error;
mod price_history_state;

use error::Result;

pub use error::SyncPriceHistoryError;
pub use price_history_state::PriceHistoryState;

pub type PriceHistoryStateTransmiter = mpsc::Sender<PriceHistoryState>;

#[derive(Clone)]
pub struct SyncPriceHistoryTask {
    config: SyncPriceHistoryTaskConfig,
    db: Arc<DbContext>,
    api: Arc<ApiClient>,
    history_state_tx: Option<PriceHistoryStateTransmiter>,
}

impl SyncPriceHistoryTask {
    pub fn new(
        config: &SyncProcessConfig,
        db: Arc<DbContext>,
        api: Arc<ApiClient>,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
    ) -> Self {
        Self {
            config: config.into(),
            db,
            api,
            history_state_tx,
        }
    }

    async fn get_new_price_entries(
        &self,
        from_observed_time: Option<DateTime<Utc>>,
        to_observed_time: Option<DateTime<Utc>>,
    ) -> Result<(Vec<PriceEntryLNM>, bool)> {
        let mut price_entries = {
            let mut trials = 0;
            loop {
                time::sleep(self.config.api_cooldown()).await;

                match self
                    .api
                    .rest
                    .futures
                    .price_history(
                        None,
                        to_observed_time,
                        Some(self.config.api_history_batch_size()),
                    )
                    .await
                {
                    Ok(price_entries) => break price_entries,
                    Err(error) => {
                        trials += 1;
                        if trials >= self.config.api_error_max_trials() {
                            return Err(SyncPriceHistoryError::RestApiMaxTrialsReached {
                                error,
                                trials: self.config.api_error_max_trials(),
                            });
                        }

                        time::sleep(self.config.api_error_cooldown()).await;
                        continue;
                    }
                };
            }
        };

        // Remove entries with duplicated 'time'
        let mut seen = HashSet::new();
        price_entries.retain(|price_entry| seen.insert(price_entry.time()));

        let is_sorted_time_desc = price_entries.is_sorted_by(|a, b| a.time() > b.time());
        if !is_sorted_time_desc {
            return Err(SyncPriceHistoryError::PriceEntriesUnsorted);
        }

        // If `before_observed_time` is set, ensure that the first (latest) entry matches it
        if let Some(observed_time) = to_observed_time {
            let first_entry = price_entries.remove(0);
            if first_entry.time() != observed_time {
                return Err(SyncPriceHistoryError::PriceEntriesWithoutOverlap);
            }
        }

        let from_observed_time_received = if let Some(time) = from_observed_time {
            if let Some(entry_i) = price_entries
                .iter()
                .position(|price_entry| price_entry.time() <= time)
            {
                // Remove the entries before the `limit`
                let before_limit = price_entries.split_off(entry_i);
                let overlap = before_limit.first().expect("not empty").time() == time;

                if !overlap {
                    return Err(SyncPriceHistoryError::FromObservedTimeNotReceived(time));
                }

                true
            } else {
                false
            }
        } else {
            false
        };

        Ok((price_entries, from_observed_time_received))
    }

    async fn partial_download(
        &self,
        from_observed_time: Option<DateTime<Utc>>,
        to_observed_time: Option<DateTime<Utc>>,
    ) -> Result<bool> {
        let (new_price_entries, from_observed_time_received) = self
            .get_new_price_entries(from_observed_time, to_observed_time)
            .await?;

        if !new_price_entries.is_empty() {
            self.db
                .price_history
                .add_entries(&new_price_entries, to_observed_time)
                .await?;
        }

        if from_observed_time_received {
            // `next` property of `from_observed_time` entry needs to be updated

            // If there is a `to_observed_time`, the first entry received from the server matched
            // its time (upper overlap enforcement).
            // From this we can infer that, if no new entries were received, there are no entries
            // to be fetched between `from_observed_time` and `to_observed_time` (edge case).
            let next_from_observed_time = new_price_entries
                .last()
                .map(|earliest_new_entry| earliest_new_entry.time())
                .or_else(|| to_observed_time);

            if let Some(next) = next_from_observed_time {
                self.db
                    .price_history
                    .update_entry_next(from_observed_time.expect("from received"), next)
                    .await?;
            }
        }

        Ok(!new_price_entries.is_empty())
    }

    async fn handle_history_update(&self, new_history_state: &PriceHistoryState) -> Result<()> {
        if let Some(history_state_tx) = self.history_state_tx.as_ref() {
            history_state_tx
                .send(new_history_state.clone())
                .await
                .map_err(|_| SyncPriceHistoryError::HistoryUpdateHandlerFailed)?;
        }

        Ok(())
    }

    pub async fn backfill(self) -> Result<()> {
        let mut history_state =
            PriceHistoryState::evaluate_with_reach(&self.db, self.config.sync_history_reach())
                .await?;
        self.handle_history_update(&history_state).await?;

        loop {
            let (download_from, download_to) = history_state.next_download_range(true)?;

            let new_entries_received = self.partial_download(download_from, download_to).await?;
            if !new_entries_received {
                if history_state.has_gaps()? {
                    return Err(SyncPriceHistoryError::NoGapEntriesReceived);
                } else {
                    if let Some(bound_end) = history_state.bound_end() {
                        // Synced with full history. Remove redundant price ticks
                        self.db.price_ticks.remove_ticks(bound_end).await?;
                    }

                    return Ok(());
                }
            }

            history_state =
                PriceHistoryState::evaluate_with_reach(&self.db, self.config.sync_history_reach())
                    .await?;
            self.handle_history_update(&history_state).await?;
        }
    }

    pub async fn live(self, range: Duration) -> Result<()> {
        if range > self.config.sync_history_reach() {
            return Err(SyncPriceHistoryError::InvalidLiveRange {
                range,
                sync_history_reach: self.config.sync_history_reach(),
            });
        }

        let history_state = PriceHistoryState::evaluate(&self.db).await?;
        self.handle_history_update(&history_state).await?;

        let initial_bound_end = history_state.bound_end();

        self.partial_download(initial_bound_end, None).await?;

        // Now it can be assumed that the history upper bound matches the current time

        loop {
            let history_state = PriceHistoryState::evaluate(&self.db).await?;
            self.handle_history_update(&history_state).await?;

            if let Some(lastest_history_range) = history_state.tail_continuous_duration() {
                if lastest_history_range >= range {
                    return Ok(());
                }
            }

            let (download_from, download_to) = history_state.next_download_range(false)?;

            self.partial_download(download_from, download_to).await?;
        }
    }
}
