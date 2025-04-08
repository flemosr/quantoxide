use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc::Receiver, Mutex},
    task::JoinHandle,
    time,
};

use crate::{
    api::{
        websocket::models::{ConnectionState, LnmWebSocketChannel, WebSocketApiRes},
        ApiContext,
    },
    db::DbContext,
    error::{AppError, Result},
};

mod price_history_task;

use price_history_task::{HistoryStateReceiver, PriceHistoryState, SyncPriceHistoryTask};

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
        let sync_reach = Utc::now() - Duration::weeks(sync_history_reach_weeks as i64);

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

    fn price_history_task(&self) -> (SyncPriceHistoryTask, HistoryStateReceiver) {
        SyncPriceHistoryTask::new(
            self.api_cooldown,
            self.api_error_cooldown,
            self.api_error_max_trials,
            self.api_history_max_entries,
            self.sync_reach,
            self.db.clone(),
            self.api.clone(),
        )
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

    fn handle_history_state_updates(&self, mut history_state_rx: Receiver<PriceHistoryState>) {
        let sync_state = self.sync_state.clone();
        let sync_tx = self.sync_tx.clone();
        tokio::spawn(async move {
            while let Some(new_history_state) = history_state_rx.recv().await {
                let mut sync_state = sync_state.lock().await;
                if let SyncState::InProgress(_) = *sync_state {
                    let new_sync_state = SyncState::InProgress(new_history_state);
                    *sync_state = new_sync_state.clone();
                    if sync_tx.receiver_count() > 0 {
                        let _ = sync_tx.send(new_sync_state).map_err(|_| {
                            AppError::Generic("couldn't send sync update".to_string())
                        });
                    }
                }
            }
        });
    }
    pub async fn start(&self) -> Result<()> {
        // Initial price history sync
        let (sync_price_history_task, history_state_rx) = self.price_history_task();

        self.handle_history_state_updates(history_state_rx);

        sync_price_history_task.run().await?;

        // Start to collect real-time data
        let mut real_time_collection_handle = self.start_real_time_collection().await?;

        // Additional price history sync to ensure overlap with real-time data
        let (sync_price_history_task, history_state_rx) = self.price_history_task();

        self.handle_history_state_updates(history_state_rx);

        sync_price_history_task.run().await?;

        // Sync achieved
        self.handle_sync_state_update(SyncState::Synced).await?;

        loop {
            let (sync_price_history_task, _) = self.price_history_task();

            tokio::select! {
                res = &mut real_time_collection_handle => {

                }
                _ = time::sleep(time::Duration::from_secs(60)) => {
                    sync_price_history_task.run().await?;
                    continue;
                }
            }
        }
    }
}
