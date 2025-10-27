use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Duration;
use tokio::{sync::broadcast, time};

use lnm_sdk::api::ApiContext;

use crate::{
    db::DbContext,
    signal::{core::ConfiguredSignalEvaluator, engine::LiveSignalEngine},
    sync::{SyncEngine, SyncMode},
    tui::{Result as TuiResult, TuiControllerShutdown, TuiError},
    util::AbortOnDropHandle,
};

use super::{
    super::core::{RawOperator, SignalOperator, WrappedRawOperator},
    config::{LiveConfig, LiveControllerConfig},
    error::{LiveError, Result},
    executor::LiveTradeExecutorLauncher,
    process::{
        LiveProcess,
        error::{LiveProcessFatalError, Result as LiveProcessResult},
        operator::OperatorPending,
    },
    state::{LiveReader, LiveReceiver, LiveStatus, LiveStatusManager, LiveTransmiter, LiveUpdate},
};

pub struct LiveController {
    config: LiveControllerConfig,
    process_handle: Mutex<Option<AbortOnDropHandle<LiveProcessResult<()>>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<LiveStatusManager>,
}

impl LiveController {
    fn new(
        config: &LiveConfig,
        process_handle: AbortOnDropHandle<LiveProcessResult<()>>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<LiveStatusManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            process_handle: Mutex::new(Some(process_handle)),
            shutdown_tx,
            status_manager,
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

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<LiveProcessResult<()>>> {
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

        // Handle may be finished due to fatal errors

        if handle.is_finished() {
            let status = self.status_manager.status_snapshot();
            return Err(LiveError::LiveAlreadyTerminated(status));
        }

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

        if let Err(e) = live_shutdown_res {
            let e_ref = Arc::new(e);
            self.status_manager.update(e_ref.clone().into());

            return Err(LiveError::LiveShutdownFailed(e_ref));
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

pub struct LiveEngine {
    config: LiveConfig,
    sync_engine: SyncEngine,
    trade_executor_launcher: LiveTradeExecutorLauncher,
    operator_pending: OperatorPending,
    status_manager: Arc<LiveStatusManager>,
    update_tx: LiveTransmiter,
}

impl LiveEngine {
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
        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let process_handle = LiveProcess::new(
            &self.config,
            shutdown_tx.clone(),
            self.sync_engine,
            self.operator_pending,
            self.trade_executor_launcher,
            self.status_manager.clone(),
            self.update_tx,
        )
        .await?
        .spawn_recovery_loop();

        let controller = LiveController::new(
            &self.config,
            process_handle,
            shutdown_tx,
            self.status_manager,
        );

        Ok(controller)
    }
}
