use chrono::{DateTime, Duration, Utc};

use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc::Receiver, Mutex},
    time,
};

use crate::{
    api::ApiContext,
    db::DbContext,
    error::{AppError, Result},
};

mod price_history_task;
mod real_time_collection_task;

use price_history_task::{HistoryStateReceiver, PriceHistoryState, SyncPriceHistoryTask};
use real_time_collection_task::RealTimeCollectionTask;

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

    fn real_time_collection_task(&self) -> RealTimeCollectionTask {
        RealTimeCollectionTask::new(self.db.clone(), self.api.clone())
    }

    fn handle_history_state_updates(&self, mut history_state_rx: Receiver<PriceHistoryState>) {
        let sync_state = self.sync_state.clone();
        let sync_tx = self.sync_tx.clone();
        tokio::spawn(async move {
            while let Some(new_history_state) = history_state_rx.recv().await {
                let mut sync_state = sync_state.lock().await;
                if let SyncState::NotInitiated | SyncState::InProgress(_) = *sync_state {
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

        let real_time_collection_task = self.real_time_collection_task();
        let mut real_time_collection_task_handle = tokio::spawn(real_time_collection_task.run());

        // Additional price history sync to ensure overlap with real-time data

        let (sync_price_history_task, history_state_rx) = self.price_history_task();

        self.handle_history_state_updates(history_state_rx);

        sync_price_history_task.run().await?;

        if real_time_collection_task_handle.is_finished() {
            real_time_collection_task_handle
                .await
                .map_err(|e| AppError::Generic(format!("join error {}", e.to_string())))?
                .map_err(|e| {
                    AppError::Generic(format!("real-time collection error {}", e.to_string()))
                })?;

            return Err(AppError::Generic(
                "unexpected real-time collection shutdown".to_string(),
            ));
        }

        // Sync achieved

        self.handle_sync_state_update(SyncState::Synced).await?;

        loop {
            tokio::select! {
                res = &mut real_time_collection_task_handle => {
                    res.map_err(|e| AppError::Generic(format!("join error {}", e.to_string())))?
                        .map_err(|e| AppError::Generic(format!("real-time collection error {}", e.to_string())))?;
                    return Err(AppError::Generic("unexpected real-time collection shutdown".to_string()));

                }
                _ = time::sleep(time::Duration::from_secs(30)) => {
                    let (sync_price_history_task, _) = self.price_history_task();
                    sync_price_history_task.run().await?;
                    continue;
                }
            }
        }
    }
}
