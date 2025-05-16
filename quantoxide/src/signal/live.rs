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
    util::DateTimeExt,
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
    Aborted,
}

pub type LiveSignalTransmiter = broadcast::Sender<Arc<LiveSignalState>>;
pub type LiveSignalReceiver = broadcast::Receiver<Arc<LiveSignalState>>;

#[derive(Clone)]
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

    async fn try_send_state_update(&self, new_state: Arc<LiveSignalState>) -> Result<()> {
        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(SignalError::SignalTransmiterFailed)?;
        }

        Ok(())
    }

    pub async fn update(&self, new_state: LiveSignalState) -> Result<()> {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        if **state_guard == *new_state {
            return Ok(());
        }

        *state_guard = new_state.clone();
        drop(state_guard);

        self.try_send_state_update(new_state).await
    }
}

struct LiveSignalProcess {
    config: LiveSignalConfig,
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
    state_manager: LiveSignalStateManager,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
}

impl LiveSignalProcess {
    fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        state_manager: LiveSignalStateManager,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(SignalError::Generic("empty `evaluators`".to_string()));
        }

        Ok(Self {
            config,
            db,
            sync_controller,
            state_manager,
            evaluators,
        })
    }

    async fn run(&self) -> Result<()> {
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
                    .await?;

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
                .price_history
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
                    .await?;
            }
        }
    }
}

pub struct LiveSignalController {
    state_manager: LiveSignalStateManager,
    handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
}

impl LiveSignalController {
    fn new(state_manager: LiveSignalStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub fn receiver(&self) -> LiveSignalReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<LiveSignalState> {
        match self.handle.lock().await.as_ref() {
            Some(handle) if handle.is_finished() => {
                return Arc::new(LiveSignalState::Failed(SignalError::Generic(
                    "Signal job process terminated unexpectedly".to_string(),
                )));
            }
            None => {
                return Arc::new(LiveSignalState::Failed(SignalError::Generic(
                    "Signal job process has been aborted".to_string(),
                )));
            }
            _ => self.state_manager.snapshot().await,
        }
    }

    /// Aborts the signal process and consumes the task handle.
    /// This method can only be called once per controller instance.
    /// Returns the result of the aborted signal process.
    pub async fn abort(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            if !handle.is_finished() {
                handle.abort();
                self.state_manager.update(LiveSignalState::Aborted).await?;
            }

            return handle.await.map_err(SignalError::TaskJoin)?;
        }

        return Err(SignalError::Generic(
            "Signal job process was already aborted".to_string(),
        ));
    }
}

#[derive(Clone, Debug)]
pub struct LiveSignalConfig {
    eval_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for LiveSignalConfig {
    fn default() -> Self {
        Self {
            eval_interval: time::Duration::from_secs(1),
            restart_interval: time::Duration::from_secs(10),
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

    pub fn set_eval_interval(mut self, secs: u64) -> Self {
        self.eval_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }
}

impl From<&LiveTradeConfig> for LiveSignalConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        Self {
            eval_interval: value.signal_eval_interval(),
            restart_interval: value.restart_interval(),
        }
    }
}

pub struct LiveSignalEngine {
    state_manager: LiveSignalStateManager,
    process: LiveSignalProcess,
    restart_interval: time::Duration,
}

impl LiveSignalEngine {
    pub fn new(
        config: LiveSignalConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    ) -> Result<Self> {
        let state_manager = LiveSignalStateManager::new();
        let restart_interval = config.restart_interval;
        let process = LiveSignalProcess::new(
            config,
            db,
            sync_controller,
            state_manager.clone(),
            evaluators,
        )?;

        Ok(Self {
            state_manager,
            process,
            restart_interval,
        })
    }

    async fn process_recovery_loop(self) -> Result<()> {
        loop {
            self.state_manager.update(LiveSignalState::Starting).await?;

            if let Err(e) = self.process.run().await {
                self.state_manager
                    .update(LiveSignalState::Failed(e))
                    .await?
            }

            self.state_manager
                .update(LiveSignalState::Restarting)
                .await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<LiveSignalController>> {
        let state_manager = self.state_manager.clone();
        let handle = tokio::spawn(self.process_recovery_loop());

        let signal_controller = LiveSignalController::new(state_manager, handle);

        Ok(Arc::new(signal_controller))
    }
}
