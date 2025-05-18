use std::sync::Arc;

use chrono::Duration;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
    time,
};

use lnm_sdk::api::ApiContext;

use crate::{db::DbContext, trade::live::LiveTradeConfig, util::Never};

pub mod error;
mod real_time_collection_task;
mod sync_price_history_task;

use error::{Result, SyncError};
use real_time_collection_task::RealTimeCollectionTask;
use sync_price_history_task::{
    PriceHistoryState, PriceHistoryStateTransmiter, SyncPriceHistoryTask,
};

#[derive(Debug, PartialEq, Eq)]
pub enum SyncState {
    NotInitiated,
    Starting,
    InProgress(PriceHistoryState),
    Synced,
    Failed(SyncError),
    Restarting,
    ShutdownInitiated,
    Shutdown,
}

pub type SyncTransmiter = broadcast::Sender<Arc<SyncState>>;
pub type SyncReceiver = broadcast::Receiver<Arc<SyncState>>;

#[derive(Clone)]
struct SyncStateManager {
    state: Arc<Mutex<Arc<SyncState>>>,
    state_tx: SyncTransmiter,
}

impl SyncStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(SyncState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<SyncState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<SyncState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.state_tx.subscribe()
    }

    async fn send_state_update(&self, new_state: Arc<SyncState>) {
        // We can safely ignore errors since they only mean that there are no
        // receivers.
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: SyncState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        *state_guard = new_state.clone();
        drop(state_guard);

        self.send_state_update(new_state).await;
    }

    pub async fn handle_history_state_updates(
        self,
        mut history_state_rx: mpsc::Receiver<PriceHistoryState>,
    ) {
        while let Some(new_history_state) = history_state_rx.recv().await {
            let mut state_guard = self.state.lock().await;
            if let SyncState::Starting | SyncState::InProgress(_) = **state_guard {
                let new_state = Arc::new(SyncState::InProgress(new_history_state));

                *state_guard = new_state.clone();
                drop(state_guard);

                self.send_state_update(new_state).await;
            }
        }
    }
}

#[derive(Clone)]
struct SyncProcess {
    config: SyncConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    state_manager: SyncStateManager,
    shutdown_tx: broadcast::Sender<()>,
}

impl SyncProcess {
    pub fn new(
        config: SyncConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        state_manager: SyncStateManager,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            config,
            db,
            api,
            state_manager,
            shutdown_tx,
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
        RealTimeCollectionTask::new(self.db.clone(), self.api.clone(), self.shutdown_tx.clone())
    }

    pub async fn run(&self) -> Result<Never> {
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
            real_time_handle.await.map_err(SyncError::TaskJoin)??;

            return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
        }

        // Sync achieved

        self.state_manager.update(SyncState::Synced).await;

        loop {
            tokio::select! {
                rt_res = &mut real_time_handle => {
                    rt_res.map_err(SyncError::TaskJoin)??;
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
    handle: Mutex<Option<JoinHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: SyncStateManager,
}

impl SyncController {
    fn new(
        handle: JoinHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        shutdown_timeout: time::Duration,
        state_manager: SyncStateManager,
    ) -> Self {
        Self {
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        }
    }

    pub fn receiver(&self) -> SyncReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<SyncState> {
        self.state_manager.snapshot().await
    }

    /// Tries to perform a clean shutdown of the sync process and consumes the
    /// task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(mut handle) = handle_guard.take() {
            if let Err(e) = self.shutdown_tx.send(()) {
                handle.abort();

                self.state_manager.update(SyncState::Shutdown).await;

                return Err(SyncError::Generic(format!(
                    "Failed to send shutdown request, {e}",
                )));
            }

            self.state_manager
                .update(SyncState::ShutdownInitiated)
                .await;

            let shutdown_res = tokio::select! {
                join_res = &mut handle => {
                    join_res.map_err(SyncError::TaskJoin)
                }
                _ = time::sleep(self.shutdown_timeout) => {
                    handle.abort();
                    Err(SyncError::Generic("Shutdown timeout".to_string()))
                }
            };

            self.state_manager.update(SyncState::Shutdown).await;
            return shutdown_res;
        }

        return Err(SyncError::Generic(
            "Sync process handle was already shutdown".to_string(),
        ));
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
    shutdown_timeout: time::Duration,
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
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl SyncConfig {
    pub fn api_cooldown(&self) -> time::Duration {
        self.api_cooldown
    }

    pub fn api_error_cooldown(&self) -> time::Duration {
        self.api_error_cooldown
    }

    pub fn api_error_max_trials(&self) -> u32 {
        self.api_error_max_trials
    }

    pub fn api_history_batch_size(&self) -> usize {
        self.api_history_batch_size
    }

    pub fn sync_history_reach(&self) -> Duration {
        self.sync_history_reach
    }

    pub fn re_sync_history_interval(&self) -> time::Duration {
        self.re_sync_history_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

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

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

impl From<&LiveTradeConfig> for SyncConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        SyncConfig {
            api_cooldown: value.api_cooldown(),
            api_error_cooldown: value.api_error_cooldown(),
            api_error_max_trials: value.api_error_max_trials(),
            api_history_batch_size: value.api_history_batch_size(),
            sync_history_reach: value.sync_history_reach(),
            re_sync_history_interval: value.re_sync_history_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

#[derive(Clone)]
pub struct SyncEngine {
    state_manager: SyncStateManager,
    process: SyncProcess,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
    shutdown_tx: broadcast::Sender<()>,
}

impl SyncEngine {
    pub fn new(config: SyncConfig, db: Arc<DbContext>, api: Arc<ApiContext>) -> Self {
        let state_manager = SyncStateManager::new();

        let restart_interval = config.restart_interval();
        let shutdown_timeout = config.shutdown_timeout();

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let process = SyncProcess::new(config, db, api, state_manager.clone(), shutdown_tx.clone());

        Self {
            state_manager,
            process,
            restart_interval,
            shutdown_timeout,
            shutdown_tx,
        }
    }

    async fn process_recovery_loop(self) {
        loop {
            self.state_manager.update(SyncState::Starting).await;

            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::select! {
                run_res = self.process.run() => {
                    let Err(sync_error) = run_res;
                    self.state_manager.update(SyncState::Failed(sync_error)).await;
                }
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        self.state_manager.update(SyncState::Failed(SyncError::ShutdownRecv(e))).await;
                    }
                    return;
                }
            };

            self.state_manager.update(SyncState::Restarting).await;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<SyncController>> {
        let state_manager = self.state_manager.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let shutdown_timeout = self.shutdown_timeout;

        let handle = tokio::spawn(self.process_recovery_loop());

        let sync_controller =
            SyncController::new(handle, shutdown_tx, shutdown_timeout, state_manager);

        Ok(Arc::new(sync_controller))
    }
}
