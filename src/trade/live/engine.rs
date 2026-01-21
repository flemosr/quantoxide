use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use lnm_sdk::{api_v2::WebSocketClient, api_v3::RestClient};

use crate::{
    db::Database,
    signal::{LiveSignalEngine, Signal, SignalEvaluator},
    sync::SyncEngine,
    tui::{
        TuiControllerShutdown,
        error::{Result as TuiResult, TuiError},
    },
    util::AbortOnDropHandle,
};

use super::{
    super::core::{Raw, RawOperator, SignalOperator, WrappedRawOperator},
    config::{LiveTradeConfig, LiveTradeControllerConfig},
    error::{LiveError, Result},
    executor::LiveTradeExecutorLauncher,
    process::{
        LiveProcess,
        error::{LiveProcessFatalError, LiveProcessFatalResult},
        operator::OperatorPending,
    },
    state::{
        LiveTradeReader, LiveTradeReceiver, LiveTradeStatus, LiveTradeStatusManager,
        LiveTradeUpdate,
    },
};

/// Controller for managing and monitoring a running live trading process. Provides an interface to
/// monitor status, receive updates, and perform graceful shutdown operations.
pub struct LiveTradeController<S: Signal> {
    config: LiveTradeControllerConfig,
    process_handle: Mutex<Option<AbortOnDropHandle<LiveProcessFatalResult<()>>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<LiveTradeStatusManager<S>>,
}

impl<S: Signal> LiveTradeController<S> {
    fn new(
        config: &LiveTradeConfig,
        process_handle: AbortOnDropHandle<LiveProcessFatalResult<()>>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<LiveTradeStatusManager<S>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            process_handle: Mutex::new(Some(process_handle)),
            shutdown_tx,
            status_manager,
        })
    }

    /// Returns a [`LiveTradeReader`] interface for accessing live status and updates.
    pub fn reader(&self) -> Arc<dyn LiveTradeReader<S>> {
        self.status_manager.clone()
    }

    /// Creates a new [`LiveTradeReceiver`] for subscribing to live trading status and updates.
    pub fn update_receiver(&self) -> LiveTradeReceiver<S> {
        self.status_manager.update_receiver()
    }

    /// Returns the current [`LiveTradeStatus`] as a snapshot.
    pub fn status_snapshot(&self) -> LiveTradeStatus {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<LiveProcessFatalResult<()>>> {
        self.process_handle
            .lock()
            .expect("`LiveTradeController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the live trade process and consumes the task handle. If
    /// a clean shutdown fails, the process is aborted.
    ///
    /// This method can only be called once per controller instance.
    ///
    /// Returns an error if the process had to be aborted, or if it the handle was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(LiveError::LiveAlreadyShutdown);
        };

        if handle.is_finished() {
            let status = self.status_manager.status_snapshot();
            return Err(LiveError::LiveAlreadyTerminated(status));
        }

        self.status_manager
            .update(LiveTradeStatus::ShutdownInitiated);

        let live_shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            LiveProcessFatalError::SendShutdownSignalFailed(e)
        });

        let live_shutdown_res = match live_shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(LiveProcessFatalError::LiveProcessTaskJoin).and_then(|r| r)
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

        self.status_manager.update(LiveTradeStatus::Shutdown);
        Ok(())
    }

    /// Waits until the live trade process has stopped and returns the final status.
    ///
    /// This method blocks until the live trade process reaches a stopped state, either through
    /// graceful shutdown or termination.
    pub async fn until_stopped(&self) -> LiveTradeStatus {
        let mut trade_rx = self.update_receiver();

        let status = self.status_snapshot();
        if status.is_stopped() {
            return status;
        }

        loop {
            match trade_rx.recv().await {
                Ok(trade_update) => {
                    if let LiveTradeUpdate::Status(status) = trade_update
                        && status.is_stopped()
                    {
                        return status;
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let status = self.status_snapshot();
                    if status.is_stopped() {
                        return status;
                    }
                }
                Err(RecvError::Closed) => return self.status_snapshot(),
            }
        }
    }
}

#[async_trait]
impl<S: Signal> TuiControllerShutdown for LiveTradeController<S> {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::LiveShutdownFailed)
    }
}

/// Builder for configuring and starting a live trading engine. Encapsulates the configuration,
/// database connection, API clients, sync engine, trade executor, and operator. The live trading
/// process is started when [`start`](Self::start) is called, returning a [`LiveTradeController`].
pub struct LiveTradeEngine<S: Signal> {
    config: LiveTradeConfig,
    sync_engine: SyncEngine,
    trade_executor_launcher: LiveTradeExecutorLauncher,
    operator_pending: OperatorPending<S>,
    status_manager: Arc<LiveTradeStatusManager<S>>,
}

impl<S: Signal> LiveTradeEngine<S> {
    /// Creates a new live trading engine using signal-based evaluation. Signal evaluators generate
    /// trading signals that are processed by the signal operator to execute trading actions.
    ///
    /// The generic parameter `S` ensures type safety between evaluators and operator. They must
    /// produce and consume the same signal type.
    #[allow(clippy::too_many_arguments)]
    pub fn with_signal_operator(
        config: LiveTradeConfig,
        db: Arc<Database>,
        api_domain: impl ToString,
        api_key: impl ToString,
        api_secret: impl ToString,
        api_passphrase: impl ToString,
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        operator: Box<dyn SignalOperator<S>>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(LiveError::EmptyEvaluatorsVec);
        }

        let api_rest = RestClient::with_credentials(
            &config,
            api_domain.to_string(),
            api_key.to_string(),
            api_secret.to_string(),
            api_passphrase.to_string(),
        )
        .map_err(LiveError::RestApiInit)?;

        let api_ws = WebSocketClient::new(&config, api_domain.to_string());
        let sync_engine = if config.sync_mode_full() {
            SyncEngine::full(&config, db.clone(), api_rest.clone(), api_ws)
        } else {
            // Find the evaluator requiring the most historical data
            let max_lookback = evaluators
                .iter()
                .filter_map(|evaluator| evaluator.lookback())
                .max_by_key(|lookback| lookback.as_duration());

            match max_lookback {
                Some(lookback) => SyncEngine::live_with_lookback(
                    &config,
                    db.clone(),
                    api_rest.clone(),
                    api_ws,
                    lookback,
                ),
                None => SyncEngine::live_no_lookback(&config, db.clone(), api_ws),
            }
        };

        let signal_engine =
            LiveSignalEngine::new(&config, db.clone(), sync_engine.reader(), evaluators)
                .map_err(LiveError::LaunchLiveSignalEngine)?;

        let operator_pending = OperatorPending::signal(signal_engine, operator.into());

        let trade_executor_launcher =
            LiveTradeExecutorLauncher::new(&config, db, api_rest, sync_engine.reader())
                .map_err(LiveError::SetupExecutor)?;

        let (update_tx, _) = broadcast::channel::<LiveTradeUpdate<S>>(1_000);

        let status_manager = LiveTradeStatusManager::new(update_tx);

        Ok(Self {
            config,
            sync_engine,
            trade_executor_launcher,
            operator_pending,
            status_manager,
        })
    }

    /// Returns a [`LiveTradeReader`] interface for accessing live status and updates.
    pub fn reader(&self) -> Arc<dyn LiveTradeReader<S>> {
        self.status_manager.clone()
    }

    /// Creates a new [`LiveTradeReceiver`] for subscribing to live trading status and updates.
    pub fn update_receiver(&self) -> LiveTradeReceiver<S> {
        self.status_manager.update_receiver()
    }

    /// Returns the current [`LiveTradeStatus`]s as a snapshot.
    pub fn status_snapshot(&self) -> LiveTradeStatus {
        self.status_manager.status_snapshot()
    }

    /// Starts the live trading process and returns a [`LiveTradeController`] for managing it. This
    /// consumes the engine and spawns the live trading task in the background.
    pub async fn start(self) -> Result<Arc<LiveTradeController<S>>> {
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let process_handle = LiveProcess::spawn(
            &self.config,
            shutdown_tx.clone(),
            self.sync_engine,
            self.operator_pending,
            self.trade_executor_launcher,
            self.status_manager.clone(),
        );

        let controller = LiveTradeController::new(
            &self.config,
            process_handle,
            shutdown_tx,
            self.status_manager,
        );

        Ok(controller)
    }
}

impl LiveTradeEngine<Raw> {
    /// Creates a new live trading engine using a raw operator. The raw operator directly implements
    /// trading logic without intermediate signal generation.
    pub fn with_raw_operator(
        config: LiveTradeConfig,
        db: Arc<Database>,
        api_domain: impl ToString,
        api_key: impl ToString,
        api_secret: impl ToString,
        api_passphrase: impl ToString,
        operator: Box<dyn RawOperator>,
    ) -> Result<Self> {
        let operator = WrappedRawOperator::from(operator);

        let api_rest = RestClient::with_credentials(
            &config,
            api_domain.to_string(),
            api_key.to_string(),
            api_secret.to_string(),
            api_passphrase.to_string(),
        )
        .map_err(LiveError::RestApiInit)?;

        let api_ws = WebSocketClient::new(&config, api_domain.to_string());
        let sync_engine = if config.sync_mode_full() {
            SyncEngine::full(&config, db.clone(), api_rest.clone(), api_ws)
        } else {
            match operator.lookback().map_err(LiveError::SetupOperatorError)? {
                Some(lookback) => SyncEngine::live_with_lookback(
                    &config,
                    db.clone(),
                    api_rest.clone(),
                    api_ws,
                    lookback,
                ),
                None => SyncEngine::live_no_lookback(&config, db.clone(), api_ws),
            }
        };

        let operator_pending = OperatorPending::raw(db.clone(), sync_engine.reader(), operator);

        let trade_executor_launcher =
            LiveTradeExecutorLauncher::new(&config, db, api_rest, sync_engine.reader())
                .map_err(LiveError::SetupExecutor)?;

        let (update_tx, _) = broadcast::channel::<LiveTradeUpdate<Raw>>(1_000);

        let status_manager = LiveTradeStatusManager::new(update_tx);

        Ok(Self {
            config,
            sync_engine,
            trade_executor_launcher,
            operator_pending,
            status_manager,
        })
    }
}
