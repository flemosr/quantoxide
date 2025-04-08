use chrono::{DateTime, Duration, Utc};
use price_history_state::PriceHistoryState;
use std::{collections::HashSet, sync::Arc};
use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
    time,
};

use crate::{
    api::{
        rest::models::PriceEntryLNM,
        websocket::models::{ConnectionState, LnmWebSocketChannel, WebSocketApiRes},
        ApiContext,
    },
    db::DbContext,
    error::{AppError, Result},
};

pub mod price_history_state;

pub type SyncTransmiter = broadcast::Sender<SyncState>;
pub type SyncReceiver = broadcast::Receiver<SyncState>;

#[derive(Clone)]
pub enum SyncState {
    NotInitiated,
    InProgress(PriceHistoryState),
    Synced,
    Failed,
}

pub struct Sync {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_max_entries: usize,
    sync_reach: DateTime<Utc>,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    sync_state: Arc<Mutex<SyncState>>,
    sync_tx: SyncTransmiter,
}

impl Sync {
    pub fn new(
        api_cooldown_sec: u64,
        api_error_cooldown_sec: u64,
        api_error_max_trials: u32,
        api_history_max_entries: usize,
        sync_history_reach_weeks: u64,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
    ) -> Self {
        // let sync_reach = Utc::now() - Duration::weeks(sync_history_reach_weeks as i64);
        let sync_reach = Utc::now() - Duration::days(2);

        // External channel for sync updates
        let (sync_tx, _) = broadcast::channel::<SyncState>(100);

        Self {
            api_cooldown: time::Duration::from_secs(api_cooldown_sec),
            api_error_cooldown: time::Duration::from_secs(api_error_cooldown_sec),
            api_error_max_trials,
            api_history_max_entries,
            sync_reach,
            db,
            api,
            sync_state: Arc::new(Mutex::new(SyncState::NotInitiated)),
            sync_tx,
        }
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.sync_tx.subscribe()
    }

    async fn handle_sync_state_update(&self, new_sync_state: SyncState) -> Result<()> {
        let mut sync_state = self.sync_state.lock().await;
        *sync_state = new_sync_state;

        if self.sync_tx.receiver_count() > 0 {
            self.sync_tx
                .send(sync_state.clone())
                .map_err(|_| AppError::Generic("couldn't send sync update".to_string()))?;
        }

        Ok(())
    }

    async fn handle_price_history_update(
        &self,
        new_history_state: &PriceHistoryState,
    ) -> Result<()> {
        let sync_state = self.sync_state.lock().await;
        if let SyncState::InProgress(_) = *sync_state {
            let new_sync_state = SyncState::InProgress(new_history_state.clone());
            self.handle_sync_state_update(new_sync_state).await?;
        }
        Ok(())
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

    async fn sync_price_history(&self) -> Result<()> {
        let mut history_state = PriceHistoryState::evaluate(&self.db, self.sync_reach).await?;
        self.handle_price_history_update(&history_state).await?;

        loop {
            let (download_from, download_to) = history_state.next_download_bounds();

            let new_entries_received = self.partial_download(download_from, download_to).await?;
            if !new_entries_received {
                if history_state.has_gaps() {
                    return Err(AppError::Generic(
                        "no entries received while gaps still exist".to_string(),
                    ));
                } else {
                    break;
                }
            }

            history_state = PriceHistoryState::evaluate(&self.db, self.sync_reach).await?;
            self.handle_price_history_update(&history_state).await?;
        }

        Ok(())
    }

    async fn start_real_time_collection(&self) -> Result<JoinHandle<()>> {
        let ws = self.api.connect_ws().await?;

        let mut receiver = ws.receiver().await?;

        let sync_state = self.sync_state.clone();
        let sync_tx = self.sync_tx.clone();
        let handle = tokio::spawn(async move {
            while let Ok(res) = receiver.recv().await {
                match res {
                    WebSocketApiRes::PriceTick(_tick) => {
                        // TODO
                    }
                    WebSocketApiRes::PriceIndex(_index) => {}
                    WebSocketApiRes::ConnectionUpdate(new_state) => match new_state {
                        ConnectionState::Connected => {}
                        ConnectionState::Disconnected => {}
                        ConnectionState::Failed(_err) => {
                            let mut sync_state = sync_state.lock().await;
                            *sync_state = SyncState::Failed;

                            if sync_tx.receiver_count() > 0 {
                                let _ = sync_tx.send(sync_state.clone());
                            }
                        }
                    },
                }
            }
            println!("Receiver closed");
        });

        let channels = vec![LnmWebSocketChannel::FuturesBtcUsdLastPrice];
        ws.subscribe(channels).await?;

        Ok(handle)
    }

    pub async fn start(&self) -> Result<()> {
        // Initial price history sync
        self.sync_price_history().await?;

        // Start to collect real-time data
        let _real_time_collection_handle = self.start_real_time_collection().await?;

        // Additional price history sync to ensure overlap with real-time data
        self.sync_price_history().await?;

        // Sync achieved
        self.handle_sync_state_update(SyncState::Synced).await?;

        Ok(())
    }
}
