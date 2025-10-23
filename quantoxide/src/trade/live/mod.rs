use std::{
    fmt,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use tokio::{sync::broadcast, time};

use lnm_sdk::api::{
    ApiContext,
    rest::models::{BoundedPercentage, LnmTrade},
};

use crate::{
    db::DbContext,
    signal::{
        core::{ConfiguredSignalEvaluator, Signal},
        engine::{LiveSignalController, LiveSignalEngine},
        state::{LiveSignalStatus, LiveSignalStatusNotRunning, LiveSignalUpdate},
    },
    sync::{
        SyncController, SyncEngine, SyncMode, SyncReader, SyncStatus, SyncStatusNotSynced,
        SyncUpdate,
    },
    tui::{Result as TuiResult, TuiControllerShutdown, TuiError},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::core::{
    RawOperator, SignalOperator, TradeExecutor, TradingState, WrappedRawOperator,
    WrappedSignalOperator,
};

pub mod error;
pub mod executor;

use error::{LiveError, Result};
use executor::{
    LiveTradeExecutor, LiveTradeExecutorLauncher,
    state::{LiveTradeExecutorStatus, LiveTradeExecutorStatusNotReady},
    update::{LiveTradeExecutorReceiver, LiveTradeExecutorUpdate, LiveTradeExecutorUpdateOrder},
};

#[derive(Debug)]
pub enum LiveStatus {
    NotInitiated,
    Starting,
    WaitingForSync(Arc<SyncStatusNotSynced>),
    WaitingForSignal(Arc<LiveSignalStatusNotRunning>),
    WaitingTradeExecutor(Arc<LiveTradeExecutorStatusNotReady>),
    Running,
    Failed(LiveError),
    Restarting,
    ShutdownInitiated,
    Shutdown,
}

impl fmt::Display for LiveStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::WaitingForSync(status) => write!(f, "Waiting for sync ({status})"),
            Self::WaitingForSignal(status) => write!(f, "Waiting for signal ({status})"),
            Self::WaitingTradeExecutor(status) => {
                write!(f, "Waiting trade executor ({status})")
            }
            Self::Running => write!(f, "Running"),
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Restarting => write!(f, "Restarting"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
        }
    }
}

#[derive(Clone)]
pub enum LiveUpdate {
    Status(Arc<LiveStatus>),
    Signal(Signal),
    Order(LiveTradeExecutorUpdateOrder),
    TradingState(TradingState),
    ClosedTrade(LnmTrade),
}

impl From<Arc<LiveStatus>> for LiveUpdate {
    fn from(value: Arc<LiveStatus>) -> Self {
        Self::Status(value)
    }
}

impl From<LiveTradeExecutorUpdateOrder> for LiveUpdate {
    fn from(value: LiveTradeExecutorUpdateOrder) -> Self {
        Self::Order(value)
    }
}

impl From<Signal> for LiveUpdate {
    fn from(value: Signal) -> Self {
        Self::Signal(value)
    }
}

impl From<TradingState> for LiveUpdate {
    fn from(value: TradingState) -> Self {
        Self::TradingState(value)
    }
}

pub type LiveTransmiter = broadcast::Sender<LiveUpdate>;
pub type LiveReceiver = broadcast::Receiver<LiveUpdate>;

pub trait LiveReader: Send + Sync + 'static {
    fn update_receiver(&self) -> LiveReceiver;
    fn status_snapshot(&self) -> Arc<LiveStatus>;
}

#[derive(Debug)]
struct LiveStatusManager {
    status: Mutex<Arc<LiveStatus>>,
    update_tx: LiveTransmiter,
}

impl LiveStatusManager {
    pub fn new(update_tx: LiveTransmiter) -> Arc<Self> {
        let status = Mutex::new(Arc::new(LiveStatus::NotInitiated));

        Arc::new(Self { status, update_tx })
    }

    fn update_status_guard(
        &self,
        mut status_guard: MutexGuard<'_, Arc<LiveStatus>>,
        new_status: LiveStatus,
    ) {
        let new_status = Arc::new(new_status);

        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    fn lock_status(&self) -> MutexGuard<'_, Arc<LiveStatus>> {
        self.status
            .lock()
            .expect("`LiveStatusManager` mutex can't be poisoned")
    }
    pub fn update(&self, new_status: LiveStatus) {
        let status_guard = self.lock_status();

        self.update_status_guard(status_guard, new_status);
    }

    pub fn update_if_not_running(&self, new_status: LiveStatus) {
        let status_guard = self.lock_status();

        if matches!(status_guard.as_ref(), LiveStatus::Running) {
            return;
        }

        self.update_status_guard(status_guard, new_status);
    }

    pub fn update_if_running(&self, new_status: LiveStatus) {
        let status_guard = self.lock_status();

        if !matches!(status_guard.as_ref(), LiveStatus::Running) {
            return;
        }

        self.update_status_guard(status_guard, new_status);
    }
}

impl LiveReader for LiveStatusManager {
    fn update_receiver(&self) -> LiveReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> Arc<LiveStatus> {
        self.lock_status().clone()
    }
}

enum OperatorRunning {
    Signal {
        signal_controller: Arc<LiveSignalController>,
        signal_operator: WrappedSignalOperator,
    },
    Raw {
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl OperatorRunning {
    fn signal_controller(&self) -> Option<Arc<LiveSignalController>> {
        if let OperatorRunning::Signal {
            signal_operator: _,
            signal_controller,
        } = self
        {
            Some(signal_controller.clone())
        } else {
            None
        }
    }
}

struct LiveProcessConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
}

impl From<&LiveConfig> for LiveProcessConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout,
            restart_interval: value.restart_interval,
        }
    }
}

struct LiveProcess {
    config: LiveProcessConfig,
    shutdown_tx: broadcast::Sender<()>,
    operator: OperatorRunning,
    trade_executor: Arc<LiveTradeExecutor>,
    status_manager: Arc<LiveStatusManager>,
    update_tx: LiveTransmiter,
}

impl LiveProcess {
    pub fn new(
        config: &LiveConfig,
        shutdown_tx: broadcast::Sender<()>,
        operator: OperatorRunning,
        trade_executor: Arc<LiveTradeExecutor>,
        status_manager: Arc<LiveStatusManager>,
        update_tx: LiveTransmiter,
    ) -> Self {
        Self {
            config: config.into(),
            shutdown_tx,
            operator,
            trade_executor,
            status_manager,
            update_tx,
        }
    }

    async fn handle_raw_entries(
        &self,
        db: &DbContext,
        sync_reader: &dyn SyncReader,
        raw_operator: &WrappedRawOperator,
    ) -> Result<Never> {
        let mut last_eval = Utc::now();

        loop {
            let iteration_interval = raw_operator
                .iteration_interval()
                .map_err(LiveError::OperatorError)?;

            let now = {
                let target_exec = (last_eval + iteration_interval).ceil_sec();
                let now = Utc::now();

                if now >= target_exec {
                    return Err(LiveError::OperatorIterationTimeTooLong);
                }

                let wait_duration = (target_exec - now).to_std().expect("valid duration");
                time::sleep(wait_duration).await;
                last_eval = target_exec;

                target_exec
            };

            if let SyncStatus::NotSynced(sync_status_not_synced) = sync_reader.status_snapshot() {
                self.status_manager
                    .update(LiveStatus::WaitingForSync(sync_status_not_synced).into());

                let mut sync_rx = sync_reader.update_receiver();
                loop {
                    tokio::select! {
                        sync_update_result = sync_rx.recv() => {
                            match sync_update_result {
                                Ok(sync_update) => match sync_update {
                                    SyncUpdate::Status(sync_status) => match sync_status {
                                        SyncStatus::NotSynced(sync_status_not_synced) => {
                                            self.status_manager.update(
                                                LiveStatus::WaitingForSync(sync_status_not_synced)
                                                    .into(),
                                            );
                                        }
                                        SyncStatus::Synced => break,
                                        SyncStatus::Terminated(err) => {
                                            return Err(LiveError::SyncProcessTerminated(err));
                                        }
                                        SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                            return Err(LiveError::SyncProcessShutdown);
                                        }
                                    },
                                    SyncUpdate::PriceTick(_) => break,
                                    SyncUpdate::PriceHistoryState(_) => {
                                        // TODO: Improve feedback on price history updates
                                        // Sync may take a long time when `sync_mode_full: true`
                                    }
                                },
                                Err(e) => return Err(LiveError::SyncRecv(e))

                            }
                        }
                        _ = time::sleep(self.config.sync_update_timeout) => {
                            if matches!(sync_reader.status_snapshot(), SyncStatus::Synced) {
                                break;
                            }
                        }
                    }
                }

                last_eval = Utc::now();
                continue;
            }

            self.status_manager
                .update_if_not_running(LiveStatus::Running);

            let ctx_window = raw_operator
                .context_window_secs()
                .map_err(LiveError::OperatorError)?;

            let ctx_entries = db
                .price_ticks
                .compute_locf_entries_for_range(now, ctx_window)
                .await?;

            raw_operator
                .iterate(ctx_entries.as_slice())
                .await
                .map_err(LiveError::OperatorError)?;
        }
    }

    async fn handle_signals(
        &self,
        signal_controller: &LiveSignalController,
        signal_operator: &WrappedSignalOperator,
    ) -> Result<Never> {
        loop {
            match signal_controller.update_receiver().recv().await {
                Ok(signal_update) => match signal_update {
                    LiveSignalUpdate::Status(signal_status) => match signal_status {
                        LiveSignalStatus::NotRunning(signal_status_not_running) => {
                            self.status_manager
                                .update(LiveStatus::WaitingForSignal(signal_status_not_running));
                        }
                        LiveSignalStatus::Running => {}
                        LiveSignalStatus::Terminated(err) => {
                            return Err(LiveError::LiveSignalProcessTerminated(err));
                        }
                        LiveSignalStatus::ShutdownInitiated | LiveSignalStatus::Shutdown => {
                            return Err(LiveError::LiveSignalProcessShutdown);
                        }
                    },
                    LiveSignalUpdate::Signal(new_signal) => {
                        let tex_state = self.trade_executor.state_snapshot().await;

                        if let LiveTradeExecutorStatus::Ready = tex_state.status() {
                            // Sync is ok, signal is ok and trade controller is ok

                            self.status_manager
                                .update_if_not_running(LiveStatus::Running);
                        } else {
                            continue;
                        }

                        // Send Signal update
                        let _ = self.update_tx.send(new_signal.clone().into());

                        signal_operator
                            .process_signal(&new_signal)
                            .await
                            .map_err(LiveError::OperatorError)?;
                    }
                },
                Err(e) => return Err(LiveError::SignalRecv(e)),
            }
        }
    }

    fn run_operator(&self) -> Pin<Box<dyn Future<Output = Result<Never>> + Send + '_>> {
        match &self.operator {
            OperatorRunning::Raw {
                db,
                sync_reader,
                raw_operator,
            } => Box::pin(self.handle_raw_entries(db, sync_reader.as_ref(), raw_operator)),
            OperatorRunning::Signal {
                signal_controller,
                signal_operator,
            } => Box::pin(self.handle_signals(signal_controller, signal_operator)),
        }
    }

    pub fn spawn_recovery_loop(self) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                self.status_manager.update(LiveStatus::Starting);

                let mut shutdown_rx = self.shutdown_tx.subscribe();

                tokio::select! {
                    run_operator_res = self.run_operator() => {
                        let Err(e) = run_operator_res;
                        self.status_manager.update(LiveStatus::Failed(e));
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(LiveStatus::Failed(
                                LiveError::ShutdownRecv(e))
                            );
                        }
                        return;
                    }
                };

                self.status_manager.update(LiveStatus::Restarting);

                // Handle shutdown signals while waiting for `restart_interval`

                tokio::select! {
                    _ = time::sleep(self.config.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(LiveStatus::Failed(
                                LiveError::ShutdownRecv(e))
                            );
                        }
                        return;
                    }
                }
            }
        })
        .into()
    }
}

#[derive(Debug)]
struct LiveControllerConfig {
    shutdown_timeout: time::Duration,
}

impl From<&LiveConfig> for LiveControllerConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

pub struct LiveController {
    config: LiveControllerConfig,
    sync_controller: Arc<SyncController>,
    signal_controller: Option<Arc<LiveSignalController>>,
    _executor_updates_handle: AbortOnDropHandle<()>,
    process_handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<LiveStatusManager>,
    trade_executor: Arc<LiveTradeExecutor>,
}

impl LiveController {
    fn new(
        config: &LiveConfig,
        sync_controller: Arc<SyncController>,
        signal_controller: Option<Arc<LiveSignalController>>,
        _executor_updates_handle: AbortOnDropHandle<()>,
        process_handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<LiveStatusManager>,
        trade_executor: Arc<LiveTradeExecutor>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            sync_controller,
            signal_controller,
            _executor_updates_handle,
            process_handle: Mutex::new(Some(process_handle)),
            shutdown_tx,
            status_manager,
            trade_executor,
        })
    }

    pub fn reader(&self) -> Arc<dyn LiveReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> LiveReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> Arc<LiveStatus> {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.process_handle
            .lock()
            .expect("`LiveController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the live trade process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(LiveError::LiveAlreadyShutdown);
        };

        self.status_manager.update(LiveStatus::ShutdownInitiated);

        // Stop live trade process

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            LiveError::SendShutdownFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(LiveError::TaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout) => {
                        handle.abort();
                        Err(LiveError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        let executor_shutdown_res = self
            .trade_executor
            .shutdown()
            .await
            .map_err(LiveError::ExecutorShutdownError);

        let signal_shutdown_res = if let Some(signal_controller) = &self.signal_controller {
            signal_controller
                .shutdown()
                .await
                .map_err(LiveError::LiveSignalShutdown)
        } else {
            Ok(())
        };

        let sync_shutdown_res = self
            .sync_controller
            .shutdown()
            .await
            .map_err(LiveError::SyncShutdown);

        self.status_manager.update(LiveStatus::Shutdown);

        shutdown_res
            .and(executor_shutdown_res)
            .and(signal_shutdown_res)
            .and(sync_shutdown_res)
    }
}

#[async_trait]
impl TuiControllerShutdown for LiveController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::LiveShutdownFailed)
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
    sync_update_timeout: time::Duration,
    tsl_step_size: BoundedPercentage,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    clean_up_trades_on_shutdown: bool,
    estimated_fee_perc: BoundedPercentage,
    max_running_qtd: usize,
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
            sync_update_timeout: time::Duration::from_secs(5),
            tsl_step_size: BoundedPercentage::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            clean_up_trades_on_shutdown: true,
            estimated_fee_perc: BoundedPercentage::try_from(0.1)
                .expect("must be valid `BoundedPercentage`"),
            max_running_qtd: 50,
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

    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn estimated_fee_perc(&self) -> BoundedPercentage {
        self.estimated_fee_perc
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
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

    pub fn set_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn set_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn set_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn set_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }

    pub fn set_estimated_fee_perc(mut self, estimated_fee_perc: BoundedPercentage) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn set_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
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

enum OperatorPending {
    Signal {
        signal_engine: LiveSignalEngine,
        signal_operator: WrappedSignalOperator,
    },
    Raw {
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl OperatorPending {
    fn signal(signal_engine: LiveSignalEngine, signal_operator: WrappedSignalOperator) -> Self {
        Self::Signal {
            signal_engine,
            signal_operator,
        }
    }

    fn raw(
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    ) -> Self {
        Self::Raw {
            db,
            sync_reader,
            raw_operator,
        }
    }

    fn start(self, trade_executor: Arc<dyn TradeExecutor>) -> Result<OperatorRunning> {
        match self {
            OperatorPending::Signal {
                signal_engine,
                signal_operator: mut operator,
            } => {
                operator
                    .set_trade_executor(trade_executor)
                    .map_err(LiveError::OperatorError)?;

                let signal_controller = signal_engine.start();

                Ok(OperatorRunning::Signal {
                    signal_operator: operator,
                    signal_controller,
                })
            }
            OperatorPending::Raw {
                db,
                sync_reader,
                mut raw_operator,
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(LiveError::OperatorError)?;

                Ok(OperatorRunning::Raw {
                    db,
                    sync_reader,
                    raw_operator,
                })
            }
        }
    }
}

pub struct LiveEngine {
    config: LiveConfig,
    sync_engine: SyncEngine,
    trade_executor_launcher: LiveTradeExecutorLauncher,
    operator_pending: OperatorPending,
    status_manager: Arc<LiveStatusManager>,
    update_tx: LiveTransmiter,
}

impl LiveEngine {
    fn spawn_executor_update_handler(
        status_manager: Arc<LiveStatusManager>,
        update_tx: LiveTransmiter,
        mut executor_rx: LiveTradeExecutorReceiver,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                match executor_rx.recv().await {
                    Ok(executor_update) => match executor_update {
                        LiveTradeExecutorUpdate::Status(executor_status) => match executor_status {
                            LiveTradeExecutorStatus::NotReady(executor_state_not_ready) => {
                                let new_status =
                                    LiveStatus::WaitingTradeExecutor(executor_state_not_ready);
                                status_manager.update_if_running(new_status.into());
                            }
                            LiveTradeExecutorStatus::Ready => {}
                        },
                        LiveTradeExecutorUpdate::Order(executor_update_order) => {
                            let _ = update_tx.send(executor_update_order.into());
                        }
                        LiveTradeExecutorUpdate::TradingState(trading_state) => {
                            let _ = update_tx.send(trading_state.into());
                        }
                        LiveTradeExecutorUpdate::ClosedTrade(closed_trade) => {
                            let _ = update_tx.send(LiveUpdate::ClosedTrade(closed_trade));
                        }
                    },
                    Err(e) => {
                        // TODO: Non-recoverable error
                        let new_status = LiveStatus::Failed(LiveError::ExecutorRecv(e));
                        status_manager.update(new_status);
                        break;
                    }
                }
            }
        })
        .into()
    }

    pub fn with_signal_operator(
        config: LiveConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: Box<dyn SignalOperator>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(LiveError::EmptyEvaluatorsVec);
        }

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

        let sync_engine = SyncEngine::new(&config, db.clone(), api.clone(), sync_mode);

        let signal_engine = LiveSignalEngine::new(
            &config,
            db.clone(),
            sync_engine.reader(),
            Arc::new(evaluators),
        )
        .map_err(LiveError::LaunchLiveSignalEngine)?;

        let operator_pending = OperatorPending::signal(signal_engine, operator.into());

        let trade_executor_launcher =
            LiveTradeExecutorLauncher::new(&config, db, api, sync_engine.update_receiver())
                .map_err(LiveError::LauchExecutor)?;

        let (update_tx, _) = broadcast::channel::<LiveUpdate>(100);

        let status_manager = LiveStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            sync_engine,
            trade_executor_launcher,
            operator_pending,
            status_manager,
            update_tx,
        })
    }

    pub fn with_raw_operator(
        config: LiveConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        operator: Box<dyn RawOperator>,
    ) -> Result<Self> {
        let operator = WrappedRawOperator::from(operator);

        let sync_mode = if config.sync_mode_full() {
            SyncMode::Full
        } else {
            let context_window_secs = operator
                .context_window_secs()
                .map_err(LiveError::OperatorError)?;

            SyncMode::Live {
                range: Duration::seconds(context_window_secs as i64),
            }
        };

        let sync_engine = SyncEngine::new(&config, db.clone(), api.clone(), sync_mode);

        let operator_pending = OperatorPending::raw(db.clone(), sync_engine.reader(), operator);

        let trade_executor_launcher =
            LiveTradeExecutorLauncher::new(&config, db, api, sync_engine.update_receiver())
                .map_err(LiveError::LauchExecutor)?;

        let (update_tx, _) = broadcast::channel::<LiveUpdate>(100);

        let status_manager = LiveStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            sync_engine,
            trade_executor_launcher,
            operator_pending,
            status_manager,
            update_tx,
        })
    }

    pub fn reader(&self) -> Arc<dyn LiveReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> LiveReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> Arc<LiveStatus> {
        self.status_manager.status_snapshot()
    }

    pub async fn start(self) -> Result<Arc<LiveController>> {
        let executor_rx = self.trade_executor_launcher.update_receiver();

        let trade_executor = self
            .trade_executor_launcher
            .launch()
            .await
            .map_err(LiveError::LauchExecutor)?;

        let _executor_updates_handle = Self::spawn_executor_update_handler(
            self.status_manager.clone(),
            self.update_tx.clone(),
            executor_rx,
        );

        let sync_controller = self.sync_engine.start();

        let operator_running = self.operator_pending.start(trade_executor.clone())?;

        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let signal_controller_opt = operator_running.signal_controller();

        let process_handle = LiveProcess::new(
            &self.config,
            shutdown_tx.clone(),
            operator_running,
            trade_executor.clone(),
            self.status_manager.clone(),
            self.update_tx,
        )
        .spawn_recovery_loop();

        let controller = LiveController::new(
            &self.config,
            sync_controller,
            signal_controller_opt,
            _executor_updates_handle,
            process_handle,
            shutdown_tx,
            self.status_manager,
            trade_executor,
        );

        Ok(controller)
    }
}
