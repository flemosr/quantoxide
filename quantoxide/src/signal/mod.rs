use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
    time,
};

use crate::{
    db::{DbContext, models::PriceHistoryEntryLOCF},
    sync::{SyncController, SyncState},
    util::DateTimeExt,
};

pub mod error;
pub mod eval;

use error::{Result, SignalError};
use eval::{SignalAction, SignalEvaluator, SignalName};

#[derive(Debug, PartialEq)]
pub struct Signal {
    time: DateTime<Utc>,
    name: SignalName,
    action: SignalAction,
}

impl Signal {
    pub(crate) async fn try_evaluate(
        evaluator: &Box<dyn SignalEvaluator>,
        entries: &[PriceHistoryEntryLOCF],
    ) -> Result<Self> {
        let signal_action = evaluator
            .evaluate(entries)
            .await
            .map_err(|e| SignalError::Generic(format!("evaluator failed {}", e.to_string())))?;

        let last_ctx_entry = entries
            .last()
            .ok_or(SignalError::Generic("empty context".to_string()))?;

        let signal = Signal {
            time: last_ctx_entry.time,
            name: evaluator.name().clone(),
            action: signal_action,
        };

        Ok(signal)
    }

    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn name(&self) -> &SignalName {
        &self.name
    }

    pub fn action(&self) -> SignalAction {
        self.action
    }
}

#[derive(Debug, PartialEq)]
pub enum SignalJobState {
    NotInitiated,
    Starting,
    Running(Signal),
    WaitingForSync,
    Failed(SignalError),
    Restarting,
    Aborted,
}

pub type SignalJobTransmiter = broadcast::Sender<Arc<SignalJobState>>;
pub type SignalJobReceiver = broadcast::Receiver<Arc<SignalJobState>>;

#[derive(Clone)]
struct SignalJobStateManager {
    state: Arc<Mutex<Arc<SignalJobState>>>,
    state_tx: SignalJobTransmiter,
}

impl SignalJobStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(SignalJobState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<SignalJobState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<SignalJobState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> SignalJobReceiver {
        self.state_tx.subscribe()
    }

    async fn try_send_state_update(&self, new_state: Arc<SignalJobState>) -> Result<()> {
        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(SignalError::SignalTransmiterFailed)?;
        }

        Ok(())
    }

    pub async fn update(&self, new_state: SignalJobState) -> Result<()> {
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

struct SignalProcess {
    config: SignalJobConfig,
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
    state_manager: SignalJobStateManager,
    evaluators: Vec<Box<dyn SignalEvaluator>>,
}

impl SignalProcess {
    fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        state_manager: SignalJobStateManager,
        evaluators: Vec<Box<dyn SignalEvaluator>>,
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
                    .update(SignalJobState::WaitingForSync)
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
                    .update(SignalJobState::Running(signal))
                    .await?;
            }
        }
    }
}

pub struct SignalJobController {
    state_manager: SignalJobStateManager,
    handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
}

impl SignalJobController {
    fn new(state_manager: SignalJobStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub fn receiver(&self) -> SignalJobReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<SignalJobState> {
        match self.handle.lock().await.as_ref() {
            Some(handle) if handle.is_finished() => {
                return Arc::new(SignalJobState::Failed(SignalError::Generic(
                    "Signal job process terminated unexpectedly".to_string(),
                )));
            }
            None => {
                return Arc::new(SignalJobState::Failed(SignalError::Generic(
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
                self.state_manager.update(SignalJobState::Aborted).await?;
            }

            return handle.await.map_err(SignalError::TaskJoin)?;
        }

        return Err(SignalError::Generic(
            "Signal job process was already aborted".to_string(),
        ));
    }
}

#[derive(Clone, Debug)]
pub struct SignalJobConfig {
    eval_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for SignalJobConfig {
    fn default() -> Self {
        Self {
            eval_interval: time::Duration::from_secs(1),
            restart_interval: time::Duration::from_secs(10),
        }
    }
}

impl SignalJobConfig {
    pub fn set_eval_interval(mut self, secs: u64) -> Self {
        self.eval_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }
}

pub struct SignalJob {
    state_manager: SignalJobStateManager,
    process: SignalProcess,
    restart_interval: time::Duration,
}

impl SignalJob {
    pub fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        evaluators: Vec<Box<dyn SignalEvaluator>>,
    ) -> Result<Self> {
        let state_manager = SignalJobStateManager::new();
        let restart_interval = config.restart_interval;
        let process = SignalProcess::new(
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
            self.state_manager.update(SignalJobState::Starting).await?;

            if let Err(e) = self.process.run().await {
                self.state_manager.update(SignalJobState::Failed(e)).await?
            }

            self.state_manager
                .update(SignalJobState::Restarting)
                .await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<SignalJobController>> {
        let state_manager = self.state_manager.clone();
        let handle = tokio::spawn(self.process_recovery_loop());

        let signal_controller = SignalJobController::new(state_manager, handle);

        Ok(Arc::new(signal_controller))
    }
}
