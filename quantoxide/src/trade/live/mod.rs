use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::Duration;
use tokio::{sync::broadcast, time};
use uuid::Uuid;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{BoundedPercentage, Leverage, Price, Quantity, TradeSide},
};

use crate::{
    db::DbContext,
    signal::{
        core::{ConfiguredSignalEvaluator, Signal},
        live::{LiveSignalConfig, LiveSignalController, LiveSignalEngine, LiveSignalState},
    },
    sync::{SyncConfig, SyncController, SyncEngine, SyncMode, SyncState},
    util::{AbortOnDropHandle, Never},
};

use super::core::{Operator, TradeExecutor, TradingState, WrappedOperator};

pub mod error;
pub mod executor;

use error::{LiveError, Result};
use executor::{
    LiveTradeExecutor, LiveTradeSetup,
    state::{LiveTradeExecutorState, LiveTradeExecutorStateNotReady},
    update::{LiveTradeExecutorUpdate, LiveTradeExecutorUpdateRunning},
};

#[derive(Debug)]
pub enum LiveStateRunningUpdate {
    ProcessSignal(Signal),
    CreateNewTrade {
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        stoploss: Price,
        takeprofit: Price,
    },
    UpdateTradeStoploss {
        id: Uuid,
        stoploss: Price,
    },
    CloseTrade {
        id: Uuid,
    },
    CancelAllTrades,
    CloseAllTrades,
    State(TradingState),
}

impl From<LiveTradeExecutorUpdateRunning> for LiveStateRunningUpdate {
    fn from(value: LiveTradeExecutorUpdateRunning) -> Self {
        match value {
            LiveTradeExecutorUpdateRunning::CreateNewTrade {
                side,
                quantity,
                leverage,
                stoploss,
                takeprofit,
            } => Self::CreateNewTrade {
                side,
                quantity,
                leverage,
                stoploss,
                takeprofit,
            },
            LiveTradeExecutorUpdateRunning::UpdateTradeStoploss { id, stoploss } => {
                Self::UpdateTradeStoploss { id, stoploss }
            }
            LiveTradeExecutorUpdateRunning::CloseTrade { id } => Self::CloseTrade { id },
            LiveTradeExecutorUpdateRunning::CancelAllTrades => Self::CancelAllTrades,
            LiveTradeExecutorUpdateRunning::CloseAllTrades => Self::CloseAllTrades,
            LiveTradeExecutorUpdateRunning::State(state) => Self::State(state),
        }
    }
}

impl From<Signal> for LiveStateRunningUpdate {
    fn from(value: Signal) -> Self {
        Self::ProcessSignal(value)
    }
}

impl fmt::Display for LiveStateRunningUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProcessSignal(signal) => {
                let signal_str = signal.to_string();
                let indented_signal = signal_str
                    .lines()
                    .map(|line| format!("  {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                write!(
                    f,
                    "LiveStateRunningUpdate::ProcessSignal:\n{}",
                    indented_signal
                )
            }
            Self::CreateNewTrade {
                side,
                quantity,
                leverage,
                stoploss,
                takeprofit,
            } => {
                write!(
                    f,
                    "LiveStateRunningUpdate::CreateNewTrade:\n  side: {}\n  quantity: {}\n  leverage: {}\n  stoploss: {:.1}\n  takeprofit: {:.1}",
                    side, quantity, leverage, stoploss, takeprofit
                )
            }
            Self::UpdateTradeStoploss { id, stoploss } => {
                write!(
                    f,
                    "LiveStateRunningUpdate::UpdateTradeStoploss:\n  id: {}\n  stoploss: {:.1}",
                    id, stoploss
                )
            }
            Self::CloseTrade { id } => {
                write!(f, "LiveStateRunningUpdate::CloseTrade:\n  id: {}", id)
            }
            Self::CancelAllTrades => write!(f, "LiveStateRunningUpdate::CancelAllTrades"),
            Self::CloseAllTrades => write!(f, "LiveStateRunningUpdate::CloseAllTrades"),
            Self::State(state) => {
                let indented_state = state
                    .to_string()
                    .lines()
                    .map(|line| format!("  {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                write!(f, "LiveStateRunningUpdate::State:\n{}", indented_state)
            }
        }
    }
}

#[derive(Debug)]
pub enum LiveState {
    NotInitiated,
    Starting,
    WaitingForSync(Arc<SyncState>), // TODO: SyncState can't be 'Synced'
    WaitingForSignal(Arc<LiveSignalState>), // TODO: LiveSignalState can't be 'Running'
    WaitingTradeController(Arc<LiveTradeExecutorStateNotReady>),
    Running(LiveStateRunningUpdate),
    Failed(LiveError),
    Restarting,
    ShutdownInitiated,
    Shutdown,
}

impl From<LiveTradeExecutorUpdate> for LiveState {
    fn from(value: LiveTradeExecutorUpdate) -> Self {
        match value {
            LiveTradeExecutorUpdate::NotReady(not_ready) => Self::WaitingTradeController(not_ready),
            LiveTradeExecutorUpdate::Ready(ready) => Self::Running(ready.into()),
        }
    }
}

impl From<LiveStateRunningUpdate> for LiveState {
    fn from(value: LiveStateRunningUpdate) -> Self {
        Self::Running(value)
    }
}

pub type LiveTransmiter = broadcast::Sender<Arc<LiveState>>;
pub type LiveReceiver = broadcast::Receiver<Arc<LiveState>>;

pub trait LiveStateReader: Send + Sync + 'static {
    fn snapshot(&self) -> Arc<LiveState>;
    fn receiver(&self) -> LiveReceiver;
}

#[derive(Debug)]
struct LiveStateManager {
    state: Mutex<Arc<LiveState>>,
    state_tx: LiveTransmiter,
}

impl LiveStateManager {
    pub fn new() -> Arc<Self> {
        let state = Mutex::new(Arc::new(LiveState::NotInitiated));
        let (state_tx, _) = broadcast::channel::<Arc<LiveState>>(100);

        Arc::new(Self { state, state_tx })
    }

    fn update_state_guard(
        &self,
        mut state_guard: MutexGuard<'_, Arc<LiveState>>,
        new_state: LiveState,
    ) {
        let new_state = Arc::new(new_state);

        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.state_tx.send(new_state);
    }

    pub fn update(&self, new_state: LiveState) {
        let state_guard = self.state.lock().expect("state lock can't be poisoned");

        self.update_state_guard(state_guard, new_state);
    }

    // pub fn handle_trade_executor_update(
    //     &self,
    //     controller_update: LiveTradeControllerUpdateRunning,
    // ) {
    // }

    pub fn update_if_not_running(&self, new_state: LiveState) {
        let state_guard = self.state.lock().expect("state lock can't be poisoned");

        if matches!(state_guard.as_ref(), LiveState::Running(_)) {
            return;
        }

        self.update_state_guard(state_guard, new_state);
    }

    pub fn update_if_running(&self, new_state: LiveState) {
        let state_guard = self.state.lock().expect("state lock can't be poisoned");

        if !matches!(state_guard.as_ref(), LiveState::Running(_)) {
            return;
        }

        self.update_state_guard(state_guard, new_state);
    }
}

impl LiveStateReader for LiveStateManager {
    fn snapshot(&self) -> Arc<LiveState> {
        self.state
            .lock()
            .expect("`LiveStateManager` mutex can't be poisoned")
            .clone()
    }

    fn receiver(&self) -> LiveReceiver {
        self.state_tx.subscribe()
    }
}

struct LiveProcess {
    restart_interval: time::Duration,
    operator: WrappedOperator,
    shutdown_tx: broadcast::Sender<()>,
    signal_controller: Arc<LiveSignalController>,
    trade_executor: Arc<LiveTradeExecutor>,
    state_manager: Arc<LiveStateManager>,
}

impl LiveProcess {
    pub fn new(
        restart_interval: time::Duration,
        operator: WrappedOperator,
        shutdown_tx: broadcast::Sender<()>,
        signal_controller: Arc<LiveSignalController>,
        trade_executor: Arc<LiveTradeExecutor>,
        state_manager: Arc<LiveStateManager>,
    ) -> Self {
        Self {
            restart_interval,
            operator,
            shutdown_tx,
            signal_controller,
            trade_executor,
            state_manager,
        }
    }

    // async fn handle_controller_updates(&self) -> Result<Never> {
    //     while let Ok(controller_update) = self.trade_executor.receiver().recv().await {
    //         self.state_manager
    //             .update_if_running(controller_update.into());
    //     }

    //     Err(LiveError::Generic(
    //         "`trade_executor` job transmitter was dropped unexpectedly".to_string(),
    //     ))
    // }

    async fn handle_signals(&self) -> Result<Never> {
        while let Ok(res) = self.signal_controller.state_receiver().recv().await {
            match res.as_ref() {
                LiveSignalState::WaitingForSync(sync_state) => {
                    self.state_manager
                        .update(LiveState::WaitingForSync(sync_state.clone()));
                }
                LiveSignalState::Running(last_signal) => {
                    let tex_state = self.trade_executor.state_snapshot().await;

                    if let LiveTradeExecutorState::Ready(ready_status) = tex_state {
                        // Sync is ok, signal is ok and trade controller is ok

                        let tex_update = LiveTradeExecutorUpdate::from(
                            LiveTradeExecutorUpdateRunning::from(ready_status),
                        );

                        self.state_manager.update_if_not_running(tex_update.into());
                    } else {
                        continue;
                    }

                    let update = LiveStateRunningUpdate::from(last_signal.clone());
                    self.state_manager.update_if_running(update.into());

                    self.operator
                        .process_signal(last_signal)
                        .await
                        .map_err(|e| LiveError::Generic(e.to_string()))?;
                }
                _ => {
                    self.state_manager.update(LiveState::WaitingForSignal(res));
                }
            }
        }

        Err(LiveError::Generic(
            "Live signals job transmitter was dropped unexpectedly".to_string(),
        ))
    }

    pub fn spawn_recovery_loop(self) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                self.state_manager.update(LiveState::Starting);

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    // handle_controller_updates_res = self.handle_controller_updates() => {
                    //     let Err(e) = handle_controller_updates_res;
                    //     self.state_manager.update(LiveState::Failed(e));
                    // }
                    handle_signals_res = self.handle_signals() => {
                        let Err(e) = handle_signals_res;
                        self.state_manager.update(LiveState::Failed(e));
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.state_manager.update(LiveState::Failed(LiveError::Generic(e.to_string())));
                        }
                        return;
                    }
                };

                self.state_manager.update(LiveState::Restarting);

                // Handle shutdown signals while waiting for `restart_interval`

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    _ = time::sleep(self.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.state_manager.update(LiveState::Failed(LiveError::Generic(e.to_string())));
                        }
                        return;
                    }
                }
            }
        }).into()
    }
}

pub struct LiveController {
    sync_controller: Arc<SyncController>,
    signal_controller: Arc<LiveSignalController>,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    shutdown_timeout: time::Duration,
    state_manager: Arc<LiveStateManager>,
    trade_executor: Arc<LiveTradeExecutor>,
}

impl LiveController {
    fn new(
        sync_controller: Arc<SyncController>,
        signal_controller: Arc<LiveSignalController>,
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        shutdown_timeout: time::Duration,
        state_manager: Arc<LiveStateManager>,
        trade_executor: Arc<LiveTradeExecutor>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sync_controller,
            signal_controller,
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            shutdown_timeout,
            state_manager,
            trade_executor,
        })
    }

    pub fn state_reader(&self) -> Arc<dyn LiveStateReader> {
        self.state_manager.clone()
    }

    pub fn state_receiver(&self) -> LiveReceiver {
        self.state_manager.receiver()
    }

    pub fn state_snapshot(&self) -> Arc<LiveState> {
        self.state_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        let mut handle_guard = self
            .handle
            .lock()
            .expect("`LiveController` mutex can't be poisoned");
        handle_guard.take()
    }

    /// Tries to perform a clean shutdown of the live trade process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(LiveError::Generic(
                "Live trade process was already shutdown".to_string(),
            ));
        };

        self.state_manager.update(LiveState::ShutdownInitiated);

        // Stop live trade process

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            LiveError::Generic(format!("Failed to send shutdown request, {e}"))
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(LiveError::TaskJoin)
                    }
                    _ = time::sleep(self.shutdown_timeout) => {
                        handle.abort();
                        Err(LiveError::Generic("Shutdown timeout".to_string()))
                    }
                }
            }
            Err(e) => Err(e),
        };

        // Close and cancel all trades

        let close_all_res = self
            .trade_executor
            .close_all()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()));

        let signal_shutdown_res = self
            .signal_controller
            .shutdown()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()));

        let sync_shutdown_res = self
            .sync_controller
            .shutdown()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()));

        self.state_manager.update(LiveState::Shutdown);

        shutdown_res
            .and(close_all_res)
            .and(signal_shutdown_res)
            .and(sync_shutdown_res)
    }
}

#[derive(Clone, Debug)]
pub struct LiveConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_mode_full: bool,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    signal_eval_interval: time::Duration,
    tsl_step_size: BoundedPercentage,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            api_cooldown: time::Duration::from_secs(2),
            api_error_cooldown: time::Duration::from_secs(10),
            api_error_max_trials: 3,
            api_history_batch_size: 1000,
            sync_mode_full: false,
            sync_history_reach: Duration::hours(24 * 7 * 4),
            re_sync_history_interval: time::Duration::from_secs(300),
            signal_eval_interval: time::Duration::from_secs(1),
            tsl_step_size: BoundedPercentage::MIN,
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
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

    pub fn sync_mode_full(&self) -> bool {
        self.sync_mode_full
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

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
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

    pub fn set_sync_mode_full(mut self, sync_mode_full: bool) -> Self {
        self.sync_mode_full = sync_mode_full;
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

pub struct LiveEngine {
    config: LiveConfig,
    sync_engine: SyncEngine,
    signal_engine: LiveSignalEngine,
    trade_setup: LiveTradeSetup,
    operator: WrappedOperator,
    state_manager: Arc<LiveStateManager>,
}

impl LiveEngine {
    // pub fn spawn_trade_manager_update_handler(
    //     state_manager: Arc<LiveStateManager>,
    //     mut trade_manager_rx: LiveTradeControllerReceiver,
    // ) -> AbortOnDropHandle<()> {
    //     tokio::spawn(async move {
    //         while let Ok(controller_update) = trade_manager_rx.recv().await {
    //             // self.state_manager
    //             //     .update_if_running(controller_update.into());
    //         }

    //         let new_state = LiveState::Failed(LiveError::Generic(
    //             "`trade_executor` job transmitter was dropped unexpectedly".to_string(),
    //         ));
    //         state_manager.update(new_state);
    //     })
    //     .into()
    // }

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

        let operator = WrappedOperator::from(operator);

        let sync_mode = if config.sync_mode_full() {
            SyncMode::Full
        } else {
            let max_evaluator_window_secs = evaluators
                .iter()
                .map(|evaluator| evaluator.context_window_secs())
                .max()
                .expect("`evaluators` can't be empty");

            SyncMode::Live {
                range: Duration::seconds(max_evaluator_window_secs as i64),
            }
        };

        let sync_config = SyncConfig::from(&config);
        let sync_engine = SyncEngine::new(sync_config, db.clone(), api.clone(), sync_mode);

        let signal_config = LiveSignalConfig::from(&config);
        let signal_engine = LiveSignalEngine::new(
            signal_config,
            db.clone(),
            sync_engine.state_reader(),
            Arc::new(evaluators),
        )
        .map_err(|e| LiveError::Generic(e.to_string()))?;

        let trade_setup =
            LiveTradeSetup::new(config.tsl_step_size, db, api, sync_engine.state_receiver());

        let trade_update_rx = trade_setup.update_receiver();

        // TODO: Handle `trade_update_rx` updates

        let state_manager = LiveStateManager::new();

        Ok(Self {
            config,
            sync_engine,
            signal_engine,
            trade_setup,
            operator,
            state_manager,
        })
    }

    pub fn state_reader(&self) -> Arc<dyn LiveStateReader> {
        self.state_manager.clone()
    }

    pub fn state_receiver(&self) -> LiveReceiver {
        self.state_manager.receiver()
    }

    pub fn state_snapshot(&self) -> Arc<LiveState> {
        self.state_manager.snapshot()
    }

    pub async fn start(mut self) -> Result<Arc<LiveController>> {
        let sync_controller = self.sync_engine.start();
        let signal_controller = self.signal_engine.start();

        let trade_executor = self
            .trade_setup
            .run()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        self.operator
            .set_trade_executor(trade_executor.clone())
            .map_err(|e| {
                LiveError::Generic(format!(
                    "couldn't set the live trades manager {}",
                    e.to_string()
                ))
            })?;

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let handle = LiveProcess::new(
            self.config.restart_interval(),
            self.operator,
            shutdown_tx.clone(),
            signal_controller.clone(),
            trade_executor.clone(),
            self.state_manager.clone(),
        )
        .spawn_recovery_loop();

        let controller = LiveController::new(
            sync_controller,
            signal_controller,
            handle,
            shutdown_tx,
            self.config.shutdown_timeout(),
            self.state_manager,
            trade_executor,
        );

        Ok(controller)
    }
}
