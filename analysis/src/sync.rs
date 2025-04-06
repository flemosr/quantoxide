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
    ) -> Result<(Option<DateTime<Utc>>, bool)> {
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

        let earliest_new_time = new_price_entries
            .into_iter()
            .last()
            .map_or(None, |entry| Some(*entry.time()));

        if from_observed_time_received {
            // `next` property of `from_observed_time` entry needs to be updated

            let next_observed_time = if let Some(time) = earliest_new_time {
                Some(time)
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

        Ok((earliest_new_time, from_observed_time_received))
    }

    async fn sync_price_history(&self) -> Result<()> {
        {
            // Fetching latest entries in order to properly evaluate the sync state
            let last_entry = self.db.price_history.get_latest_entry().await?;
            let from_observed_time = last_entry.map_or(None, |entry| Some(entry.time));

            self.partial_download(from_observed_time.as_ref(), None)
                .await?;
        }

        let mut gaps: Vec<(Limit, DateTime<Utc>)> = self
            .db
            .price_history
            .get_gaps()
            .await?
            .into_iter()
            .map(|(from, to)| (Limit::Entry(from), to))
            .collect();

        if let Some((limit, _)) = gaps.first() {
            if limit.time() < &self.sync_reach {
                // There is a price gap before `sync_reach`. Since we shouldn't fetch entries
                // before `sync_reach`. Said gap can't be closed, and therefore the db can't
                // be synced.
                return Err(AppError::UnreachableDbGap {
                    earliest_gap: *limit.time(),
                    limit: self.sync_reach,
                });
            }
        }

        let earliest_entry =
            self.db
                .price_history
                .get_latest_entry()
                .await?
                .ok_or(AppError::Generic(
                    "db is empty after first partial download".to_string(),
                ))?;

        if earliest_entry.time > self.sync_reach {
            gaps.insert(0, (Limit::Reach(self.sync_reach), earliest_entry.time));
        }

        // Now we have all gaps, with time ascending

        while !gaps.is_empty() {
            // Close more recent gaps first

            let (gap_from, gap_to) = gaps.last_mut().expect("not empty");
            loop {
                let from_observed_time = match gap_from {
                    Limit::Entry(time) => Some(&*time),
                    Limit::Reach(_) => None,
                };

                let (new_gap_to, from_received) = self
                    .partial_download(from_observed_time, Some(*gap_to))
                    .await?;

                if let Some(new_gap_to) = new_gap_to {
                    let gap_closed = match gap_from {
                        Limit::Entry(_) => from_received,
                        Limit::Reach(reach_time) => new_gap_to < *reach_time,
                    };

                    if gap_closed {
                        gaps.pop();
                        break;
                    }

                    *gap_to = new_gap_to;
                } else {
                    // No new entries were received from the server

                    if from_received {
                        // No entries between `from` and `to`, edge case
                        gaps.pop();
                        break;
                    }

                    return Err(AppError::Generic(format!(
                        "server returned no entries before {gap_to}."
                    )));
                }
            }
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
