use std::sync::Arc;

use chrono::Utc;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
    time,
};

use crate::{
    db::DbContext,
    sync::{SyncController, SyncState},
    trade::live::LiveTradeConfig,
    util::{DateTimeExt, Never},
};

use super::{
    core::{ConfiguredSignalEvaluator, Signal},
    error::{Result, SignalError},
};

#[derive(Debug, PartialEq)]
pub enum LiveSignalState {
    NotInitiated,
    Starting,
    Running(Signal),
    WaitingForSync(Arc<SyncState>),
    Failed(SignalError),
    Restarting,
    ShutdownInitiated,
    Shutdown,
}

pub type LiveSignalTransmiter = broadcast::Sender<Arc<LiveSignalState>>;
pub type LiveSignalReceiver = broadcast::Receiver<Arc<LiveSignalState>>;

#[derive(Debug, Clone)]
struct LiveSignalStateManager {
    state: Arc<Mutex<Arc<LiveSignalState>>>,
    state_tx: LiveSignalTransmiter,
}

impl LiveSignalStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(LiveSignalState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<LiveSignalState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<LiveSignalState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveSignalReceiver {
        self.state_tx.subscribe()
    }

    async fn send_state_update(&self, new_state: Arc<LiveSignalState>) {
        // We can safely ignore errors since they only mean that there are no
        // receivers.
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: LiveSignalState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        *state_guard = new_state.clone();
        drop(state_guard);

        self.send_state_update(new_state).await
    }
}

struct LiveSignalProcess {
    config: LiveSignalConfig,
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    state_manager: LiveSignalStateManager,
}

impl LiveSignalProcess {
    fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
        state_manager: LiveSignalStateManager,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(SignalError::Generic("empty `evaluators`".to_string()));
        }

        Ok(Self {
            config,
            db,
            sync_controller,
            evaluators,
            state_manager,
        })
    }

    async fn run(&self) -> Result<Never> {
        let mut last_eval = Utc::now();

        loop {
            let now = {
                let target_exec = (last_eval + self.config.eval_interval).ceil_sec();
                let now = Utc::now();

                if now >= target_exec {
                    return Err(SignalError::Generic(
                        "evaluation time incompatible with eval interval".to_string(),
                    ));
                }

                let wait_duration = (target_exec - now).to_std().expect("valid duration");
                time::sleep(wait_duration).await;
                last_eval = target_exec;

                target_exec
            };

            let sync_state = self.sync_controller.state_snapshot().await;

            if *sync_state != SyncState::Synced {
                self.state_manager
                    .update(LiveSignalState::WaitingForSync(sync_state))
                    .await;

                continue;
            }

            let max_ctx_window = self
                .evaluators
                .iter()
                .map(|evaluator| evaluator.context_window_secs())
                .max()
                .expect("evaluators can't be empty");

            let all_ctx_entries = self
                .db
                .price_ticks
                .eval_entries_locf(&now, max_ctx_window)
                .await
                .map_err(|_| SignalError::Generic("db error".to_string()))?;

            let last_ctx_entry = all_ctx_entries
                .last()
                .ok_or(SignalError::Generic("empty context".to_string()))?;
            if now != last_ctx_entry.time {
                return Err(SignalError::Generic("invalid context".to_string()));
            }

            for evaluator in self.evaluators.iter() {
                let ctx_size = evaluator.context_window_secs();
                if all_ctx_entries.len() < ctx_size {
                    return Err(SignalError::Generic(
                        "evaluator with inconsistent window size".to_string(),
                    ));
                }

                let start_idx = all_ctx_entries.len() - ctx_size;
                let signal_ctx_entries = &all_ctx_entries[start_idx..];

                let signal = Signal::try_evaluate(evaluator, signal_ctx_entries).await?;

                self.state_manager
                    .update(LiveSignalState::Running(signal))
                    .await;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveSignalController {
    handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: LiveSignalStateManager,
}

impl LiveSignalController {
    fn new(
        handle: JoinHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        shutdown_timeout: time::Duration,
        state_manager: LiveSignalStateManager,
    ) -> Self {
        Self {
            handle: Arc::new(Mutex::new(Some(handle))),
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        }
    }

    pub fn receiver(&self) -> LiveSignalReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<LiveSignalState> {
        self.state_manager.snapshot().await
    }

    /// Tries to perform a clean shutdown of the live signal process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(mut handle) = handle_guard.take() {
            if let Err(e) = self.shutdown_tx.send(()) {
                handle.abort();

                self.state_manager.update(LiveSignalState::Shutdown).await;

                return Err(SignalError::Generic(format!(
                    "Failed to send shutdown request, {e}",
                )));
            }

            self.state_manager
                .update(LiveSignalState::ShutdownInitiated)
                .await;

            let shutdown_res = tokio::select! {
                join_res = &mut handle => {
                    join_res.map_err(SignalError::TaskJoin)
                }
                _ = time::sleep(self.shutdown_timeout) => {
                    handle.abort();
                    Err(SignalError::Generic("Shutdown timeout".to_string()))
                }
            };

            self.state_manager.update(LiveSignalState::Shutdown).await;
            return shutdown_res;
        }

        return Err(SignalError::Generic(
            "Live signal process was already shutdown".to_string(),
        ));
    }
}

#[derive(Clone, Debug)]
pub struct LiveSignalConfig {
    eval_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveSignalConfig {
    fn default() -> Self {
        Self {
            eval_interval: time::Duration::from_secs(1),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveSignalConfig {
    pub fn eval_interval(&self) -> time::Duration {
        self.eval_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn set_eval_interval(mut self, secs: u64) -> Self {
        self.eval_interval = time::Duration::from_secs(secs);
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

impl From<&LiveTradeConfig> for LiveSignalConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        Self {
            eval_interval: value.signal_eval_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

pub struct LiveSignalEngine {
    process: LiveSignalProcess,
    restart_interval: time::Duration,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: LiveSignalStateManager,
}

impl LiveSignalEngine {
    pub fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    ) -> Result<Self> {
        let restart_interval = config.restart_interval();
        let shutdown_timeout = config.shutdown_timeout();

        let state_manager = LiveSignalStateManager::new();

        let process = LiveSignalProcess::new(
            config,
            db,
            sync_controller,
            evaluators,
            state_manager.clone(),
        )?;

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        Ok(Self {
            process,
            restart_interval,
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        })
    }

    async fn process_recovery_loop(self) {
        loop {
            self.state_manager.update(LiveSignalState::Starting).await;

            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::select! {
                run_res = self.process.run() => {
                    let Err(signal_error) = run_res;
                    self.state_manager.update(LiveSignalState::Failed(signal_error)).await;
                }
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        self.state_manager.update(LiveSignalState::Failed(SignalError::Generic(e.to_string()))).await;
                    }
                    return;
                }
            };

            self.state_manager.update(LiveSignalState::Restarting).await;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<LiveSignalController>> {
        let shutdown_tx = self.shutdown_tx.clone();
        let shutdown_timeout = self.shutdown_timeout;
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(self.process_recovery_loop());

        let signal_controller =
            LiveSignalController::new(handle, shutdown_tx, shutdown_timeout, state_manager);

        Ok(Arc::new(signal_controller))
    }
}
