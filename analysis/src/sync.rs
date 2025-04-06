use chrono::{DateTime, Duration, Utc};
use std::{collections::HashSet, sync::Arc};
use tokio::{task::JoinHandle, time};

use crate::{
    api::{
        rest::models::PriceEntryLNM,
        websocket::models::{LnmWebSocketChannel, WebSocketApiRes},
        ApiContext,
    },
    db::DbContext,
    error::{AppError, Result},
};

enum Limit {
    Reach(DateTime<Utc>),
    Entry(DateTime<Utc>),
}

impl Limit {
    fn time(&self) -> &DateTime<Utc> {
        match self {
            Limit::Entry(time) | Limit::Reach(time) => time,
        }
    }
}

enum LimitReached {
    No,
    Yes { overlap: bool },
}

pub struct Sync {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_max_entries: usize,
    sync_reach: DateTime<Utc>,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
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
        let sync_reach = Utc::now() - Duration::weeks(sync_history_reach_weeks as i64);

        println!("\nPrice history sync reach: {sync_history_reach_weeks} weeks",);
        println!("Limit timestamp: {sync_reach}");

        Self {
            api_cooldown: time::Duration::from_secs(api_cooldown_sec),
            api_error_cooldown: time::Duration::from_secs(api_error_cooldown_sec),
            api_error_max_trials,
            api_history_max_entries,
            sync_reach,
            db,
            api,
        }
    }

    async fn get_new_price_entries(
        &self,
        from_observed_time: Option<&DateTime<Utc>>,
        to_observed_time: Option<DateTime<Utc>>,
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
                        println!("\nError fetching price history {:?}", api_error);

                        trials += 1;
                        if trials >= self.api_error_max_trials {
                            return Err(AppError::ApiMaxTrialsReached {
                                api_error,
                                max_trials: self.api_error_max_trials,
                            });
                        }

                        println!(
                            "Remaining trials: {}. Waiting {:?}...",
                            self.api_error_max_trials - trials,
                            self.api_error_cooldown
                        );

                        time::sleep(self.api_error_cooldown).await;

                        continue;
                    }
                };
            }
        };

        if price_entries.len() < self.api_history_max_entries {
            println!(
                "\nReceived only {} price entries with limit {}.",
                price_entries.len(),
                self.api_history_max_entries
            );
        }

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
            if *first_entry.time() != observed_time {
                return Err(AppError::UnexpectedLNMPayload(
                    "Price entries without overlap".to_string(),
                ));
            }
            println!(
                "First received entry matches `before_observed_time` time {}. Overlap OK.",
                observed_time
            );
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
        to_observed_time: Option<DateTime<Utc>>,
    ) -> Result<usize> {
        match to_observed_time {
            Some(time) => println!("\nFetching price entries before {time}..."),
            None => println!("\nFetching latest price entries..."),
        }

        let (new_price_entries, from_observed_time_received) = self
            .get_new_price_entries(from_observed_time, to_observed_time)
            .await?;

        if new_price_entries.is_empty() {
            println!("\nNo new entries were received.");
        } else {
            let entries_len = new_price_entries.len();
            let latest_new_entry_time = new_price_entries.first().expect("not empty").time();
            let earliest_new_entry_time = new_price_entries.last().expect("not empty").time();
            println!("\n{entries_len} new entries received, from {earliest_new_entry_time} to {latest_new_entry_time}");

            self.db
                .price_history
                .add_entries(&new_price_entries, to_observed_time.as_ref())
                .await?;

            println!("\nEntries added to the db");
        }

        if from_observed_time_received {
            // `next` property of `from_observed_time` entry needs to be updated

            let next_observed_time = if let Some(earliest_new_entry) = new_price_entries.last() {
                Some(*earliest_new_entry.time())
            } else if let Some(time) = to_observed_time {
                // If there is a `next_observed_time`, the first entry received
                // from the server matched its time (upper overlap enforcement).
                // From this, we can infer that there are no entries to be
                // fetched between `limit` and `next_observed_time` (edge case).
                Some(time)
            } else {
                // No entries available after `limit`
                None
            };

            if let Some(next) = next_observed_time {
                self.db
                    .price_history
                    .update_entry_next(from_observed_time.unwrap(), &next)
                    .await?;
            }
        }

        Ok(new_price_entries.len())
    }

    async fn sync_price_history(&self) -> Result<()> {
        let mut gaps = PriceHistoryGaps::evaluate(&self.db, &self.sync_reach).await?;

        loop {
            let (download_from, download_to) = gaps.pick_download_bounds();

            let new_entries_len = self
                .partial_download(download_from.as_ref(), download_to)
                .await?;

            gaps = PriceHistoryGaps::evaluate(&self.db, &self.sync_reach).await?;
        }

        Ok(())
    }

    async fn start_real_time_collection(&self) -> Result<JoinHandle<()>> {
        println!("\nInit WebSocket connection");

        let ws = self.api.connect_ws().await?;

        println!("\nWS running. Setting up receiver...");

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

        println!("\nReceiver running. Subscribing to prices and index...");

        ws.subscribe(vec![LnmWebSocketChannel::FuturesBtcUsdLastPrice])
            .await?;

        println!("\nSubscriptions {:?}", ws.subscriptions().await);

        Ok(handle)
    }

    pub async fn run(&self) -> Result<()> {
        // Initial price history sync
        self.sync_price_history().await?;

        // Start to collect real-time data
        let real_time_collection_handle = self.start_real_time_collection().await?;

        // Additional price history sync to ensure overlap with real-time data
        self.sync_price_history().await?;

        // Sync achieved
        println!("\nSync achieved.\n");

        real_time_collection_handle
            .await
            .map_err(|e| AppError::Generic(e.to_string()))?;

        Ok(())
    }
}

pub struct PriceHistoryGaps {
    lower_tail: Option<(DateTime<Utc>, DateTime<Utc>)>,
    entry_gaps: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    upper_bound: Option<DateTime<Utc>>,
}

impl PriceHistoryGaps {
    pub async fn evaluate(db: &DbContext, reach: &DateTime<Utc>) -> Result<Self> {
        let (earliest_entry, lastest_entry) = {
            let earliest_entry = db.price_history.get_earliest_entry().await?;
            let lastest_entry = db.price_history.get_latest_entry().await?;

            if earliest_entry.is_none() {
                // Db is empty
                return Ok(Self {
                    lower_tail: None,
                    entry_gaps: Vec::new(),
                    upper_bound: None,
                });
            }

            (
                earliest_entry.expect("db not empty"),
                lastest_entry.expect("db not empty"),
            )
        };

        if earliest_entry.time == lastest_entry.time {
            // Db has a single entry
            if earliest_entry.time < *reach {
                return Err(AppError::UnreachableDbGap {
                    gap: earliest_entry.time,
                    reach: *reach,
                });
            }

            return Ok(Self {
                lower_tail: Some((*reach, earliest_entry.time)),
                entry_gaps: Vec::new(),
                upper_bound: Some(earliest_entry.time),
            });
        }

        let lower_tail = if *reach < earliest_entry.time {
            Some((*reach, earliest_entry.time))
        } else {
            None
        };

        let entry_gaps = db.price_history.get_gaps().await?;

        if let Some((from_time, _)) = entry_gaps.first() {
            if from_time < reach {
                // There is a price gap before `reach`. Since we shouldn't fetch entries
                // before `reach`. Said gap can't be closed, and therefore the db can't
                // be synced.
                return Err(AppError::UnreachableDbGap {
                    gap: *from_time,
                    reach: *reach,
                });
            }
        }

        Ok(Self {
            lower_tail,
            entry_gaps,
            upper_bound: Some(lastest_entry.time),
        })
    }

    pub fn pick_download_bounds(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        if let Some((gap_from, gap_to)) = self.entry_gaps.first() {
            return (Some(*gap_from), Some(*gap_to));
        }
        if let Some((_, earliest_entry_time)) = self.lower_tail {
            return (None, Some(earliest_entry_time));
        }
        (self.upper_bound, None)
    }

    pub fn has_gaps(&self) -> bool {}
}
