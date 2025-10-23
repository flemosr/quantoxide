use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Duration;
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use lnm_sdk::api::ApiContext;

use crate::{
    db::DbContext,
    signal::{
        core::ConfiguredSignalEvaluator,
        engine::{LiveSignalController, LiveSignalEngine},
    },
    sync::{SyncController, SyncEngine, SyncMode, SyncReader},
    tui::{Result as TuiResult, TuiControllerShutdown, TuiError},
    util::AbortOnDropHandle,
};

use super::{
    super::core::{
        RawOperator, SignalOperator, TradeExecutor, WrappedRawOperator, WrappedSignalOperator,
    },
    config::{LiveConfig, LiveControllerConfig},
    error::{LiveError, Result},
    executor::{
        LiveTradeExecutor, LiveTradeExecutorLauncher,
        state::LiveTradeExecutorStatus,
        update::{LiveTradeExecutorReceiver, LiveTradeExecutorUpdate},
    },
    process::{
        LiveProcess, OperatorRunning,
        error::{LiveProcessFatalError, LiveProcessRecoverableError},
    },
    state::{LiveReader, LiveReceiver, LiveStatus, LiveStatusManager, LiveTransmiter, LiveUpdate},
};

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

        let live_shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            LiveProcessFatalError::SendShutdownSignalFailed(e)
        });

        let live_shutdown_res = match live_shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(LiveProcessFatalError::LiveProcessTaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout()) => {
                        handle.abort();
                        Err(LiveProcessFatalError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        let executor_shutdown_res = self
            .trade_executor
            .shutdown()
            .await
            .map_err(LiveProcessFatalError::ExecutorShutdownError);

        let signal_shutdown_res = if let Some(signal_controller) = &self.signal_controller {
            signal_controller
                .shutdown()
                .await
                .map_err(LiveProcessFatalError::LiveSignalShutdown)
        } else {
            Ok(())
        };

        let sync_shutdown_res = self
            .sync_controller
            .shutdown()
            .await
            .map_err(LiveProcessFatalError::SyncShutdown);

        let shutdown_res = live_shutdown_res
            .and(executor_shutdown_res)
            .and(signal_shutdown_res)
            .and(sync_shutdown_res);

        if let Err(err) = shutdown_res {
            let err_ref = Arc::new(err);
            self.status_manager.update(err_ref.clone().into());

            return Err(LiveError::LiveShutdownFailed(err_ref));
        }

        self.status_manager.update(LiveStatus::Shutdown);
        Ok(())
    }
}

#[async_trait]
impl TuiControllerShutdown for LiveController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::LiveShutdownFailed)
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
                    .map_err(LiveError::SetupOperatorError)?;

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
                    .map_err(LiveError::SetupOperatorError)?;

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
                    Err(RecvError::Lagged(skipped)) => {
                        let err = LiveProcessRecoverableError::ExecutorRecvLagged { skipped };
                        status_manager.update(err.into());
                    }
                    Err(RecvError::Closed) => {
                        status_manager.update(LiveProcessFatalError::ExecutorRecvClosed.into());
                        return;
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
                .map_err(LiveError::SetupOperatorError)?;

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
