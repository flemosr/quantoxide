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
    signal::{SignalJob, SignalJobConfig, SignalJobState, eval::ConfiguredSignalEvaluator},
    sync::{Sync, SyncConfig, SyncState},
    trade::{
        LiveTradesManager,
        core::{Operator, TradesManager, TradesState, WrappedOperator},
    },
};

pub mod error;

use error::{LiveError, Result};

#[derive(Debug, PartialEq)]
pub enum LiveState {
    NotInitiated,
    Starting,
    Syncing(Arc<SyncState>),
    WaitingForSync(Arc<SyncState>),
    WaitingForSignalJob(Arc<SignalJobState>),
    Running(TradesState),
    Failed(LiveError),
    Restarting,
    Aborted,
}

pub type LiveTransmiter = broadcast::Sender<Arc<LiveState>>;
pub type LiveReceiver = broadcast::Receiver<Arc<LiveState>>;

#[derive(Clone)]
struct LiveStateManager {
    state: Arc<Mutex<Arc<LiveState>>>,
    state_tx: LiveTransmiter,
}

impl LiveStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(LiveState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<LiveState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<LiveState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveReceiver {
        self.state_tx.subscribe()
    }

    async fn try_send_state_update(&self, new_state: Arc<LiveState>) -> Result<()> {
        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(|e| LiveError::Generic(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn update(&self, new_state: LiveState) -> Result<()> {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        *state_guard = new_state.clone();
        drop(state_guard);

        self.try_send_state_update(new_state).await
    }
}

struct LiveProcess {
    config: LiveConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    operator: WrappedOperator,
    state_manager: LiveStateManager,
}

impl LiveProcess {
    pub fn new(
        config: LiveConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: WrappedOperator,
        state_manager: LiveStateManager,
    ) -> Self {
        Self {
            config,
            db,
            api,
            evaluators: Arc::new(evaluators),
            operator,
            state_manager,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let config = SyncConfig::from(&self.config);
        let sync_controller = Sync::new(config, self.db.clone(), self.api.clone())
            .start()
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        while let Ok(res) = sync_controller.receiver().recv().await {
            self.state_manager
                .update(LiveState::Syncing(res.clone()))
                .await?;

            match res.as_ref() {
                SyncState::Synced => {
                    break;
                }
                SyncState::Aborted => {
                    return Err(LiveError::Generic(
                        "Sync process unexpectedly aborted".to_string(),
                    ));
                }
                _ => {}
            }
        }

        let trades_manager = {
            let manager = LiveTradesManager::new(self.api.clone())
                .await
                .map_err(|e| LiveError::Generic(e.to_string()))?;
            Arc::new(manager)
        };

        self.operator
            .set_trades_manager(trades_manager.clone())
            .map_err(|e| {
                LiveError::Generic(format!(
                    "couldn't set the live trades manager {}",
                    e.to_string()
                ))
            })?;

        let config = SignalJobConfig::from(&self.config);
        let signal_job_controller = SignalJob::new(
            config,
            self.db.clone(),
            sync_controller.clone(),
            self.evaluators.clone(),
        )
        .map_err(|e| LiveError::Generic(e.to_string()))?
        .start()
        .map_err(|e| LiveError::Generic(e.to_string()))?;

        while let Ok(res) = signal_job_controller.receiver().recv().await {
            match res.as_ref() {
                SignalJobState::Running(last_signal) => {
                    self.operator
                        .process_signal(last_signal)
                        .await
                        .map_err(|e| LiveError::Generic(e.to_string()))?;

                    let trades_state = trades_manager
                        .state()
                        .await
                        .map_err(|e| LiveError::Generic(e.to_string()))?;

                    self.state_manager
                        .update(LiveState::Running(trades_state))
                        .await?;
                }
                SignalJobState::WaitingForSync(sync_state) => {
                    self.state_manager
                        .update(LiveState::WaitingForSync(sync_state.clone()))
                        .await?;
                }
                _ => {
                    self.state_manager
                        .update(LiveState::WaitingForSignalJob(res))
                        .await?;
                }
            }
        }

        todo!()
    }
}

pub struct LiveController {
    state_manager: LiveStateManager,
    handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
}

impl LiveController {
    fn new(state_manager: LiveStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub fn receiver(&self) -> LiveReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<LiveState> {
        match self.handle.lock().await.as_ref() {
            Some(handle) if handle.is_finished() => {
                return Arc::new(LiveState::Failed(LiveError::Generic(
                    "Live process terminated unexpectedly".to_string(),
                )));
            }
            None => {
                return Arc::new(LiveState::Failed(LiveError::Generic(
                    "Live process has been aborted".to_string(),
                )));
            }
            _ => self.state_manager.snapshot().await,
        }
    }

    /// Aborts the live process and consumes the task handle.
    /// This method can only be called once per controller instance.
    /// Returns the result of the aborted sync process.
    pub async fn abort(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            if !handle.is_finished() {
                handle.abort();
                self.state_manager.update(LiveState::Aborted).await?;
            }

            return handle
                .await
                .map_err(|e| LiveError::Generic(e.to_string()))?;
        }

        return Err(LiveError::Generic(
            "Live process was already aborted".to_string(),
        ));
    }
}

#[derive(Clone, Debug)]
pub struct LiveConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    signal_eval_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for LiveConfig {
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
        }
    }
}

impl LiveConfig {
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
}

pub struct Live {
    state_manager: LiveStateManager,
    process: LiveProcess,
    restart_interval: time::Duration,
}

impl Live {
    pub fn new(
        config: LiveConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: Box<dyn Operator>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(LiveError::Generic(
                "At least one evaluator must be provided".to_string(),
            ));
        }

        let state_manager = LiveStateManager::new();
        let restart_interval = config.restart_interval;

        let process = LiveProcess::new(
            config,
            db,
            api,
            evaluators,
            operator.into(),
            state_manager.clone(),
        );

        Ok(Self {
            state_manager,
            process,
            restart_interval,
        })
    }

    async fn process_recovery_loop(mut self) -> Result<()> {
        loop {
            self.state_manager.update(LiveState::Starting).await?;

            if let Err(e) = self.process.run().await {
                self.state_manager.update(LiveState::Failed(e)).await?
            }

            self.state_manager.update(LiveState::Restarting).await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<LiveController>> {
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(self.process_recovery_loop());

        let controller = LiveController::new(state_manager, handle);

        Ok(Arc::new(controller))
    }
}
