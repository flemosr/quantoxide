use std::sync::Arc;

use chrono::Duration;
use lnm_sdk::api::ApiContext;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
    time,
};

use crate::{
    db::DbContext,
    signal::{
        core::{ConfiguredSignalEvaluator, Signal},
        live::{LiveSignalConfig, LiveSignalEngine, LiveSignalState},
    },
    sync::{SyncConfig, SyncEngine, SyncState},
    util::Never,
};

use super::core::{Operator, TradeManager, TradeManagerState, WrappedOperator};

pub mod error;
pub mod manager;

use error::{LiveTradeError, Result};
use manager::LiveTradeManager;

#[derive(Debug, PartialEq)]
pub enum LiveTradeState {
    NotInitiated,
    Starting,
    Syncing(Arc<SyncState>),
    WaitingForSync(Arc<SyncState>),
    WaitingForSignal(Arc<LiveSignalState>),
    Running((Signal, TradeManagerState)),
    Failed(LiveTradeError),
    Restarting,
    ShutdownInitiated,
    Shutdown,
}

pub type LiveTradeTransmiter = broadcast::Sender<Arc<LiveTradeState>>;
pub type LiveTradeReceiver = broadcast::Receiver<Arc<LiveTradeState>>;

#[derive(Debug, Clone)]
struct LiveTradeStateManager {
    state: Arc<Mutex<Arc<LiveTradeState>>>,
    state_tx: LiveTradeTransmiter,
}

impl LiveTradeStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(LiveTradeState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<LiveTradeState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<LiveTradeState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveTradeReceiver {
        self.state_tx.subscribe()
    }

    async fn send_state_update(&self, new_state: Arc<LiveTradeState>) {
        // We can safely ignore errors since they only mean that there are no
        // receivers.
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: LiveTradeState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        match state_guard.as_ref() {
            // Ignore eventual errors resulting from the shutdown of subprocesses
            LiveTradeState::ShutdownInitiated if *new_state != LiveTradeState::Shutdown => return,
            _ => *state_guard = new_state.clone(),
        }

        drop(state_guard);
        self.send_state_update(new_state).await
    }
}

struct LiveTradeProcess {
    config: LiveTradeConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    operator: WrappedOperator,
    shutdown_tx: broadcast::Sender<()>,
    state_manager: LiveTradeStateManager,
}

impl LiveTradeProcess {
    pub fn new(
        config: LiveTradeConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: WrappedOperator,
        shutdown_tx: broadcast::Sender<()>,
        state_manager: LiveTradeStateManager,
    ) -> Self {
        Self {
            config,
            db,
            api,
            evaluators: Arc::new(evaluators),
            operator,
            shutdown_tx,
            state_manager,
        }
    }

    pub async fn run(&mut self) -> Result<Never> {
        let config = SyncConfig::from(&self.config);
        let sync_controller = SyncEngine::new(config, self.db.clone(), self.api.clone())
            .set_external_shutdown_trigger(self.shutdown_tx.subscribe())
            .start()
            .map_err(|e| LiveTradeError::Generic(e.to_string()))?;

        while let Ok(res) = sync_controller.receiver().recv().await {
            self.state_manager
                .update(LiveTradeState::Syncing(res.clone()))
                .await;

            match res.as_ref() {
                SyncState::Synced => {
                    break;
                }
                SyncState::Shutdown => {
                    return Err(LiveTradeError::Generic(
                        "Sync process unexpectedly shutdown".to_string(),
                    ));
                }
                _ => {}
            }
        }

        let trades_manager = {
            let manager = LiveTradeManager::new(self.api.clone())
                .await
                .map_err(|e| LiveTradeError::Generic(e.to_string()))?;
            Arc::new(manager)
        };

        self.operator
            .set_trades_manager(trades_manager.clone())
            .map_err(|e| {
                LiveTradeError::Generic(format!(
                    "couldn't set the live trades manager {}",
                    e.to_string()
                ))
            })?;

        let config = LiveSignalConfig::from(&self.config);
        let signal_job_controller = LiveSignalEngine::new(
            config,
            self.db.clone(),
            sync_controller.clone(),
            self.evaluators.clone(),
        )
        .map_err(|e| LiveTradeError::Generic(e.to_string()))?
        .start()
        .map_err(|e| LiveTradeError::Generic(e.to_string()))?;

        while let Ok(res) = signal_job_controller.receiver().recv().await {
            match res.as_ref() {
                LiveSignalState::Running(last_signal) => {
                    self.operator
                        .process_signal(last_signal)
                        .await
                        .map_err(|e| LiveTradeError::Generic(e.to_string()))?;

                    let trades_state = trades_manager
                        .state()
                        .await
                        .map_err(|e| LiveTradeError::Generic(e.to_string()))?;

                    self.state_manager
                        .update(LiveTradeState::Running((last_signal.clone(), trades_state)))
                        .await;
                }
                LiveSignalState::WaitingForSync(sync_state) => {
                    self.state_manager
                        .update(LiveTradeState::WaitingForSync(sync_state.clone()))
                        .await;
                }
                _ => {
                    self.state_manager
                        .update(LiveTradeState::WaitingForSignal(res))
                        .await;
                }
            }
        }

        Err(LiveTradeError::Generic(
            "Live signals job transmitter was dropped unexpectedly".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct LiveTradeController {
    handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: LiveTradeStateManager,
}

impl LiveTradeController {
    fn new(
        handle: JoinHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        shutdown_timeout: time::Duration,
        state_manager: LiveTradeStateManager,
    ) -> Self {
        Self {
            handle: Arc::new(Mutex::new(Some(handle))),
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        }
    }

    pub fn receiver(&self) -> LiveTradeReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<LiveTradeState> {
        self.state_manager.snapshot().await
    }

    /// Tries to perform a clean shutdown of the live trade process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(mut handle) = handle_guard.take() {
            if let Err(e) = self.shutdown_tx.send(()) {
                handle.abort();

                self.state_manager.update(LiveTradeState::Shutdown).await;

                return Err(LiveTradeError::Generic(format!(
                    "Failed to send shutdown request, {e}",
                )));
            }

            self.state_manager
                .update(LiveTradeState::ShutdownInitiated)
                .await;

            let shutdown_res = tokio::select! {
                join_res = &mut handle => {
                    join_res.map_err(LiveTradeError::TaskJoin)
                }
                _ = time::sleep(self.shutdown_timeout) => {
                    handle.abort();
                    Err(LiveTradeError::Generic("Shutdown timeout".to_string()))
                }
            };

            self.state_manager.update(LiveTradeState::Shutdown).await;
            return shutdown_res;
        }

        return Err(LiveTradeError::Generic(
            "Live trade process was already shutdown".to_string(),
        ));
    }
}

#[derive(Clone, Debug)]
pub struct LiveTradeConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    signal_eval_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveTradeConfig {
    fn default() -> Self {
        Self {
            api_cooldown: time::Duration::from_secs(60),
            api_error_cooldown: time::Duration::from_secs(300),
            api_error_max_trials: 3,
            api_history_batch_size: 1000,
            sync_history_reach: Duration::hours(24),
            re_sync_history_interval: time::Duration::from_secs(3000),
            signal_eval_interval: time::Duration::from_secs(1),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveTradeConfig {
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

    pub fn signal_eval_interval(&self) -> time::Duration {
        self.signal_eval_interval
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

    pub fn set_signal_eval_interval(mut self, secs: u64) -> Self {
        self.signal_eval_interval = time::Duration::from_secs(secs);
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

pub struct LiveTradeEngine {
    process: LiveTradeProcess,
    restart_interval: time::Duration,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: LiveTradeStateManager,
}

impl LiveTradeEngine {
    pub fn new(
        config: LiveTradeConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: Box<dyn Operator>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(LiveTradeError::Generic(
                "At least one evaluator must be provided".to_string(),
            ));
        }

        let restart_interval = config.restart_interval();
        let shutdown_timeout = config.shutdown_timeout();

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let state_manager = LiveTradeStateManager::new();

        let process = LiveTradeProcess::new(
            config,
            db,
            api,
            evaluators,
            operator.into(),
            shutdown_tx.clone(),
            state_manager.clone(),
        );

        Ok(Self {
            process,
            restart_interval,
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        })
    }

    async fn process_recovery_loop(mut self) {
        loop {
            self.state_manager.update(LiveTradeState::Starting).await;

            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::select! {
                run_res = self.process.run() => {
                    let Err(e) = run_res;
                    self.state_manager.update(LiveTradeState::Failed(e)).await;
                }
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        self.state_manager.update(LiveTradeState::Failed(LiveTradeError::Generic(e.to_string()))).await;
                    }
                    return;
                }
            };

            self.state_manager.update(LiveTradeState::Restarting).await;

            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<LiveTradeController>> {
        let shutdown_tx = self.shutdown_tx.clone();
        let shutdown_timeout = self.shutdown_timeout;
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(self.process_recovery_loop());

        let controller =
            LiveTradeController::new(handle, shutdown_tx, shutdown_timeout, state_manager);

        Ok(Arc::new(controller))
    }
}
