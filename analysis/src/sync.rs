use chrono::{DateTime, Duration, Utc};
use std::{collections::HashSet, fmt, sync::Arc};
use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
    time,
};

use crate::{
    api::{
        rest::models::PriceEntryLNM,
        websocket::models::{LnmWebSocketChannel, WebSocketApiRes},
        ApiContext,
    },
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

    async fn update_sync_state(&self, new_sync_state: SyncState) -> Result<()> {
        let mut sync_state = self.sync_state.lock().await;
        *sync_state = new_sync_state.clone();
        if self.sync_tx.receiver_count() > 0 {
            self.sync_tx
                .send(new_sync_state)
                .map_err(|_| AppError::Generic("couldn't send sync update".to_string()))?;
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
        self.update_sync_state(SyncState::InProgress(history_state.clone()))
            .await?;

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
            self.update_sync_state(SyncState::InProgress(history_state.clone()))
                .await?;
        }

        Ok(())
    }

    async fn start_real_time_collection(&self) -> Result<JoinHandle<()>> {
        println!("\nInit WebSocket connection");

        let ws = self.api.connect_ws().await?;

        // println!("\nWS running. Setting up receiver...");

        let mut receiver = ws.receiver().await?;

        let handle = tokio::spawn(async move {
            while let Ok(res) = receiver.recv().await {
                match res {
                    WebSocketApiRes::PriceTick(tick) => {
                        println!("Tick received {:?}", tick);
                    }
                    WebSocketApiRes::PriceIndex(index) => {
                        println!("Index received {:?}", index);
                    }
                    WebSocketApiRes::ConnectionUpdate(new_state) => {
                        println!("Connection update received {:?}", new_state);
                    }
                }
            }
            println!("Receiver closed");
        });

        // println!("\nReceiver running. Subscribing to prices and index...");

        ws.subscribe(vec![LnmWebSocketChannel::FuturesBtcUsdLastPrice])
            .await?;

        // println!("\nSubscriptions {:?}", ws.subscriptions().await);

        Ok(handle)
    }

    pub async fn run(&self) -> Result<()> {
        // Initial price history sync
        self.sync_price_history().await?;

        // Start to collect real-time data
        let _real_time_collection_handle = self.start_real_time_collection().await?;

        // Additional price history sync to ensure overlap with real-time data
        self.sync_price_history().await?;

        // Sync achieved
        self.update_sync_state(SyncState::Synced).await?;

        // real_time_collection_handle
        //     .await
        //     .map_err(|e| AppError::Generic(e.to_string()))?;

        Ok(())
    }
}
