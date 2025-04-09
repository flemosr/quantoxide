use chrono::{DateTime, Utc};
use std::{collections::HashSet, sync::Arc};
use tokio::{sync::mpsc, time};

use crate::{
    api::{rest::models::PriceEntryLNM, ApiContext},
    db::DbContext,
    error::{AppError, Result},
};

mod price_history_state;
pub use price_history_state::PriceHistoryState;

pub type PriceHistoryStateTransmiter = mpsc::Sender<PriceHistoryState>;

#[derive(Clone)]
pub struct SyncPriceHistoryTask {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_max_entries: usize,
    sync_reach: DateTime<Utc>,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    history_state_tx: Option<PriceHistoryStateTransmiter>,
}

impl SyncPriceHistoryTask {
    pub fn new(
        api_cooldown: time::Duration,
        api_error_cooldown: time::Duration,
        api_error_max_trials: u32,
        api_history_max_entries: usize,
        sync_reach: DateTime<Utc>,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
    ) -> Self {
        let task = Self {
            api_cooldown,
            api_error_cooldown,
            api_error_max_trials,
            api_history_max_entries,
            sync_reach,
            db,
            api,
            history_state_tx,
        };

        task
    }

    async fn get_new_price_entries(
        &self,
        from_observed_time: Option<&DateTime<Utc>>,
        to_observed_time: Option<&DateTime<Utc>>,
    ) -> Result<(Vec<PriceEntryLNM>, bool)> {
        let mut price_entries = {
            let mut trials = 0;
            let rest_futures = &self.api.rest().futures;
            loop {
                time::sleep(self.api_cooldown).await;

                match rest_futures
                    .price_history(None, to_observed_time, Some(self.api_history_max_entries))
                    .await
                {
                    Ok(price_entries) => break price_entries,
                    Err(api_error) => {
                        trials += 1;
                        if trials >= self.api_error_max_trials {
                            return Err(AppError::ApiMaxTrialsReached {
                                api_error,
                                max_trials: self.api_error_max_trials,
                            });
                        }

                        time::sleep(self.api_error_cooldown).await;
                        continue;
                    }
                };
            }
        };

        // Remove entries with duplicated 'time'
        let mut seen = HashSet::new();
        price_entries.retain(|price_entry| seen.insert(*price_entry.time()));

        let is_sorted_time_desc = price_entries.is_sorted_by(|a, b| a.time() > b.time());
        if !is_sorted_time_desc {
            return Err(AppError::UnexpectedLNMPayload(
                "Price entries unsorted by time desc".to_string(),
            ));
        }

        // If `before_observed_time` is set, ensure that the first (latest) entry matches it
        if let Some(observed_time) = to_observed_time {
            let first_entry = price_entries.remove(0);
            if first_entry.time() != observed_time {
                return Err(AppError::UnexpectedLNMPayload(
                    "Price entries without overlap".to_string(),
                ));
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
                    return Err(AppError::UnexpectedLNMPayload(format!(
                        "limit entry time {} not received from server",
                        time
                    )));
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
        from_observed_time: Option<&DateTime<Utc>>,
        to_observed_time: Option<&DateTime<Utc>>,
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

            let next_observed_time = if let Some(earliest_new_entry) = new_price_entries.last() {
                Some(*earliest_new_entry.time())
            } else if let Some(time) = to_observed_time {
                // If there is a `next_observed_time`, the first entry received from the server
                // matched its time (upper overlap enforcement).
                // From this, we can infer that there are no entries to be fetched between
                // `from_observed_time` and `next_observed_time` (edge case).
                Some(*time)
            } else {
                // No entries available after `from_observed_time`
                None
            };

            if let Some(next) = next_observed_time {
                self.db
                    .price_history
                    .update_entry_next(from_observed_time.expect("from received"), &next)
                    .await?;
            }
        }

        Ok(!new_price_entries.is_empty())
    }

    async fn handle_history_update(&self, new_history_state: &PriceHistoryState) -> Result<()> {
        if let Some(history_state_tx) = self.history_state_tx.as_ref() {
            if !history_state_tx.is_closed() {
                history_state_tx
                    .send(new_history_state.clone())
                    .await
                    .map_err(|_| {
                        AppError::Generic("couldn't send price history update".to_string())
                    })?;
            }
        }

        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        let mut history_state = PriceHistoryState::evaluate(&self.db, self.sync_reach).await?;
        self.handle_history_update(&history_state).await?;

        loop {
            let (download_from, download_to) = history_state.next_download_bounds();

            let new_entries_received = self.partial_download(download_from, download_to).await?;
            if !new_entries_received {
                if history_state.has_gaps() {
                    return Err(AppError::Generic(
                        "no entries received while gaps still exist".to_string(),
                    ));
                } else {
                    return Ok(());
                }
            }

            history_state = PriceHistoryState::evaluate(&self.db, self.sync_reach).await?;
            self.handle_history_update(&history_state).await?;
        }
    }
}
