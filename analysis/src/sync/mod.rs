use chrono::Duration;
use std::sync::Arc;
use tokio::{
    sync::{
        broadcast,
        mpsc::{self, Receiver},
        Mutex,
    },
    task::JoinHandle,
    time,
};

use crate::{api::ApiContext, db::DbContext};

pub mod error;
mod real_time_collection_task;
mod sync_price_history_task;

use error::{Result, SyncError};
use real_time_collection_task::RealTimeCollectionTask;
use sync_price_history_task::{
    PriceHistoryState, PriceHistoryStateTransmiter, SyncPriceHistoryTask,
};

pub type SyncTransmiter = broadcast::Sender<SyncState>;
pub type SyncReceiver = broadcast::Receiver<SyncState>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncState {
    NotInitiated,
    Starting,
    InProgress(PriceHistoryState),
    Synced,
    Failed(String),
    Restarting,
}

#[derive(Clone)]
struct SyncStateManager {
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
        let mut state_lock = self.state.lock().await;
        *state_lock = new_state.clone();
        drop(state_lock);

        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(|e| SyncError::SyncTransmiterFailed(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn handle_history_state_updates(
        self,
        mut history_state_rx: Receiver<PriceHistoryState>,
    ) -> Result<()> {
        while let Some(new_history_state) = history_state_rx.recv().await {
            let state_lock = self.state.lock().await;
            if let SyncState::Starting | SyncState::InProgress(_) = *state_lock {
                drop(state_lock);

                let new_sync_state = SyncState::InProgress(new_history_state);
                self.update(new_sync_state).await?;
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
struct SyncProcess {
    config: SyncConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    state_manager: SyncStateManager,
}

impl SyncProcess {
    pub fn new(
        config: SyncConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        state_manager: SyncStateManager,
    ) -> Self {
        Self {
            config,
            db,
            api,
            state_manager,
        }
    }

    fn price_history_task(
        &self,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
    ) -> SyncPriceHistoryTask {
        SyncPriceHistoryTask::new(
            self.config.clone(),
            self.db.clone(),
            self.api.clone(),
            history_state_tx,
        )
    }

    fn real_time_collection_task(&self) -> RealTimeCollectionTask {
        RealTimeCollectionTask::new(self.db.clone(), self.api.clone())
    }

    pub async fn run(&self) -> Result<()> {
        // Initial price history sync

        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        let state_manager = self.state_manager.clone();
        tokio::spawn(state_manager.handle_history_state_updates(history_state_rx));

        let sync_price_history_task = self.price_history_task(Some(history_state_tx));
        sync_price_history_task.run().await?;

        // Start to collect real-time data

        let real_time_collection_task = self.real_time_collection_task();
        let mut real_time_handle = tokio::spawn(real_time_collection_task.run());

        // Additional price history sync to ensure overlap with real-time data

        let sync_price_history_task = self.price_history_task(None);
        sync_price_history_task.run().await?;

        if real_time_handle.is_finished() {
            real_time_handle
                .await
                .map_err(|e| SyncError::TaskJoin(e.to_string()))??;

            return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
        }

        // Sync achieved

        self.state_manager.update(SyncState::Synced).await?;

        loop {
            tokio::select! {
                rt_res = &mut real_time_handle => {
                    rt_res.map_err(|e| SyncError::TaskJoin(e.to_string()))??;
                    return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
                }
                _ = time::sleep(self.config.re_sync_history_interval) => {
                    let sync_price_history_task = self.price_history_task(None);
                    sync_price_history_task.run().await?;
                }
            }
        }
    }
}

pub struct SyncController {
    state_manager: SyncStateManager,
    handle: JoinHandle<Result<()>>,
}

impl SyncController {
    fn new(state_manager: SyncStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle,
        }
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.state_manager.receiver()
    }

    pub async fn state(&self) -> Result<SyncState> {
        let state = self.state_manager.state.lock().await.clone();
        Ok(state)
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

#[derive(Clone, Debug)]
pub struct SyncConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            api_cooldown: time::Duration::from_secs(60),
            api_error_cooldown: time::Duration::from_secs(300),
            api_error_max_trials: 3,
            api_history_batch_size: 1000,
            sync_history_reach: Duration::hours(24),
            re_sync_history_interval: time::Duration::from_secs(3000),
            restart_interval: time::Duration::from_secs(10),
        }
    }
}

impl SyncConfig {
    pub fn set_api_cooldown(mut self, secs: u64) -> Self {
        self.api_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_cooldown(mut self, secs: u64) -> Self {
        self.api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_max_trials(mut self, max_trials: u32) -> Self {
        self.api_error_max_trials = max_trials;
        self
    }

    pub fn set_api_history_batch_size(mut self, size: usize) -> Self {
        self.api_history_batch_size = size;
        self
    }

    pub fn set_sync_history_reach(mut self, hours: u64) -> Self {
        self.sync_history_reach = Duration::hours(hours as i64);
        self
    }

    pub fn set_re_sync_history_interval(mut self, secs: u64) -> Self {
        self.re_sync_history_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }
}

#[derive(Clone)]
pub struct Sync {
    state_manager: SyncStateManager,
    process: SyncProcess,
    restart_interval: time::Duration,
}

impl Sync {
    pub fn new(config: SyncConfig, db: Arc<DbContext>, api: Arc<ApiContext>) -> Self {
        let state_manager = SyncStateManager::new();
        let restart_interval = config.restart_interval;

        let process = SyncProcess::new(config, db, api, state_manager.clone());

        Self {
            state_manager,
            process,
            restart_interval,
        }
    }

    async fn process_recovery_loop(self) -> Result<()> {
        loop {
            self.state_manager.update(SyncState::Starting).await?;

            if let Err(e) = self.process.run().await {
                self.state_manager
                    .update(SyncState::Failed(e.to_string()))
                    .await?
            }

            self.state_manager.update(SyncState::Restarting).await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<SyncController> {
        let state_manager = self.state_manager.clone();
        let handle = tokio::spawn(self.process_recovery_loop());

        let sync_controller = SyncController::new(state_manager, handle);

        Ok(sync_controller)
    }
}
