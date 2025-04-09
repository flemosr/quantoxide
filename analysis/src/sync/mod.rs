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
    Starting,
    InProgress(PriceHistoryState),
    Synced,
    Failed,
    Restarting,
}

#[derive(Clone)]
pub struct SyncStateManager {
    state: Arc<Mutex<SyncState>>,
    state_tx: SyncTransmiter,
}

impl SyncStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(SyncState::NotInitiated));
        let (state_tx, _) = broadcast::channel::<SyncState>(100);
        Self { state, state_tx }
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.state_tx.subscribe()
    }

    pub async fn update(&self, new_state: SyncState) -> Result<()> {
        let mut state = self.state.lock().await;
        *state = new_state.clone();
        if self.state_tx.receiver_count() > 0 {
            let _ = self
                .state_tx
                .send(new_state)
                .map_err(|_| AppError::Generic("couldn't send state update".to_string()));
        }
        Ok(())
    }

    pub async fn handle_history_state_updates(
        self,
        mut history_state_rx: Receiver<PriceHistoryState>,
    ) -> Result<()> {
        while let Some(new_history_state) = history_state_rx.recv().await {
            let sync_state = self.state.lock().await;
            if let SyncState::Starting | SyncState::InProgress(_) = *sync_state {
                let new_sync_state = SyncState::InProgress(new_history_state);
                self.update(new_sync_state).await?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct SyncProcess {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_max_entries: usize,
    sync_reach: DateTime<Utc>,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    state_manager: SyncStateManager,
}

impl SyncProcess {
    pub fn new(
        api_cooldown_sec: u64,
        api_error_cooldown_sec: u64,
        api_error_max_trials: u32,
        api_history_max_entries: usize,
        sync_history_reach_weeks: u64,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        state_manager: SyncStateManager,
    ) -> Self {
        let sync_reach = Utc::now() - Duration::hours(4 as i64);
        Self {
            api_cooldown: time::Duration::from_secs(api_cooldown_sec),
            api_error_cooldown: time::Duration::from_secs(api_error_cooldown_sec),
            api_error_max_trials,
            api_history_max_entries,
            sync_reach,
            db,
            api,
            state_manager,
        }
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

    pub async fn run(&self) -> Result<()> {
        // Initial price history sync

        let (sync_price_history_task, history_state_rx) = self.price_history_task();

        let state_manager = self.state_manager.clone();
        tokio::spawn(state_manager.handle_history_state_updates(history_state_rx));

        sync_price_history_task.run().await?;

        // Start to collect real-time data

        let real_time_collection_task = self.real_time_collection_task();
        let mut real_time_collection_task_handle = tokio::spawn(real_time_collection_task.run());

        // Additional price history sync to ensure overlap with real-time data

        let (sync_price_history_task, history_state_rx) = self.price_history_task();

        let state_manager = self.state_manager.clone();
        tokio::spawn(state_manager.handle_history_state_updates(history_state_rx));

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

        self.state_manager.update(SyncState::Synced).await?;

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

#[derive(Clone)]
pub struct Sync {
    state_manager: SyncStateManager,
    process: SyncProcess,
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
        let state_manager = SyncStateManager::new();

        let process = SyncProcess::new(
            api_cooldown_sec,
            api_error_cooldown_sec,
            api_error_max_trials,
            api_history_max_entries,
            sync_history_reach_weeks,
            db,
            api,
            state_manager.clone(),
        );

        Self {
            state_manager,
            process,
        }
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.state_manager.receiver()
    }

    async fn start_inner(state_manager: SyncStateManager, process: SyncProcess) -> Result<()> {
        loop {
            state_manager.update(SyncState::Starting).await?;

            match process.run().await {
                Ok(_) => {}
                Err(_) => state_manager.update(SyncState::Failed).await?,
            }

            state_manager.update(SyncState::Restarting).await?;
            time::sleep(time::Duration::from_secs(10)).await;
        }
    }

    pub fn start(&self) -> tokio::task::JoinHandle<Result<()>> {
        let state_manager = self.state_manager.clone();
        let process = self.process.clone();

        tokio::spawn(Self::start_inner(state_manager, process))
    }
}
