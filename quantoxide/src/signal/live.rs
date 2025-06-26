use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::{sync::broadcast, time};

use crate::{
    db::DbContext,
    sync::{SyncState, SyncStateReader},
    trade::live::LiveConfig,
    util::{AbortOnDropHandle, DateTimeExt, Never},
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

#[derive(Debug)]
struct LiveSignalStateManager {
    state: Mutex<Arc<LiveSignalState>>,
    state_tx: LiveSignalTransmiter,
}

impl LiveSignalStateManager {
    pub fn new() -> Arc<Self> {
        let state = Mutex::new(Arc::new(LiveSignalState::NotInitiated));
        let (state_tx, _) = broadcast::channel::<Arc<LiveSignalState>>(100);

        Arc::new(Self { state, state_tx })
    }

    pub fn snapshot(&self) -> Arc<LiveSignalState> {
        self.state
            .lock()
            .expect("state lock can't be poisoned")
            .clone()
    }

    pub fn receiver(&self) -> LiveSignalReceiver {
        self.state_tx.subscribe()
    }

    pub fn update(&self, new_state: LiveSignalState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().expect("state lock can't be poisoned");
        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.state_tx.send(new_state);
    }
}

struct LiveSignalProcess {
    config: LiveSignalConfig,
    db: Arc<DbContext>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    shutdown_tx: broadcast::Sender<()>,
    sync_state_reader: Arc<dyn SyncStateReader>,
    state_manager: Arc<LiveSignalStateManager>,
}

impl LiveSignalProcess {
    fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
        shutdown_tx: broadcast::Sender<()>,
        sync_state_reader: Arc<dyn SyncStateReader>,
        state_manager: Arc<LiveSignalStateManager>,
    ) -> Self {
        Self {
            config,
            db,
            evaluators,
            shutdown_tx,
            sync_state_reader,
            state_manager,
        }
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

            if !matches!(
                self.sync_state_reader.snapshot().as_ref(),
                SyncState::Synced(_)
            ) {
                while let Ok(sync_state) = self.sync_state_reader.receiver().recv().await {
                    match sync_state.as_ref() {
                        SyncState::Synced(_) => break,
                        _ => {
                            self.state_manager
                                .update(LiveSignalState::WaitingForSync(sync_state));

                            continue;
                        }
                    }
                }

                last_eval = Utc::now();
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
                .compute_locf_entries_for_range(now, max_ctx_window)
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

                self.state_manager.update(LiveSignalState::Running(signal));
            }
        }
    }

    fn spawn_recovery_loop(self) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                self.state_manager.update(LiveSignalState::Starting);

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    run_res = self.run() => {
                        let Err(signal_error) = run_res;
                        self.state_manager.update(LiveSignalState::Failed(signal_error));
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.state_manager.update(LiveSignalState::Failed(SignalError::Generic(e.to_string())));
                        }
                        return;
                    }
                };

                self.state_manager.update(LiveSignalState::Restarting);

                // Handle shutdown signals while waiting for `restart_interval`

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    _ = time::sleep(self.config.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.state_manager.update(LiveSignalState::Failed(SignalError::Generic(e.to_string())));
                        }
                        return;
                    }
                }
            }
        }).into()
    }
}

#[derive(Debug)]
pub struct LiveSignalController {
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: Arc<LiveSignalStateManager>,
}

impl LiveSignalController {
    fn new(
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        shutdown_timeout: time::Duration,
        state_manager: Arc<LiveSignalStateManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            shutdown_timeout,
            state_manager,
        })
    }

    pub fn receiver(&self) -> LiveSignalReceiver {
        self.state_manager.receiver()
    }

    pub fn state_snapshot(&self) -> Arc<LiveSignalState> {
        self.state_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        let mut handle_guard = self
            .handle
            .lock()
            .expect("`LiveSignalController` mutex can't be poisoned");
        handle_guard.take()
    }

    /// Tries to perform a clean shutdown of the live signal process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(SignalError::Generic(
                "Live signal process was already shutdown".to_string(),
            ));
        };

        self.state_manager
            .update(LiveSignalState::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SignalError::Generic(format!("Failed to send shutdown request, {e}"))
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SignalError::TaskJoin)
                    }
                    _ = time::sleep(self.shutdown_timeout) => {
                        handle.abort();
                        Err(SignalError::Generic("Shutdown timeout".to_string()))
                    }
                }
            }
            Err(e) => Err(e),
        };

        self.state_manager.update(LiveSignalState::Shutdown);

        shutdown_res
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

impl From<&LiveConfig> for LiveSignalConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            eval_interval: value.signal_eval_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

pub struct LiveSignalEngine {
    config: LiveSignalConfig,
    db: Arc<DbContext>,
    sync_state_reader: Arc<dyn SyncStateReader>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
}

impl LiveSignalEngine {
    pub fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        sync_state_reader: Arc<dyn SyncStateReader>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(SignalError::Generic(
                "At least one evaluator must be provided".to_string(),
            ));
        }

        Ok(Self {
            config,
            db,
            sync_state_reader,
            evaluators,
        })
    }

    pub fn start(self) -> Arc<LiveSignalController> {
        let shutdown_timeout = self.config.shutdown_timeout;

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let state_manager = LiveSignalStateManager::new();

        let handle = LiveSignalProcess::new(
            self.config,
            self.db,
            self.evaluators,
            shutdown_tx.clone(),
            self.sync_state_reader,
            state_manager.clone(),
        )
        .spawn_recovery_loop();

        LiveSignalController::new(handle, shutdown_tx, shutdown_timeout, state_manager)
    }
}
