use std::{pin::Pin, sync::Arc};

use chrono::Utc;
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    db::DbContext,
    signal::{
        engine::LiveSignalController,
        state::{LiveSignalStatus, LiveSignalUpdate},
    },
    sync::{SyncController, SyncEngine, SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::{
    super::core::{WrappedRawOperator, WrappedSignalOperator},
    config::{LiveConfig, LiveProcessConfig},
    executor::{
        LiveTradeExecutor, LiveTradeExecutorLauncher,
        state::LiveTradeExecutorStatus,
        update::{LiveTradeExecutorReceiver, LiveTradeExecutorUpdate},
    },
    state::{LiveStatus, LiveStatusManager, LiveTransmiter, LiveUpdate},
};

pub mod error;
pub mod operator;

use error::{
    LiveProcessError, LiveProcessFatalError, LiveProcessFatalResult, LiveProcessRecoverableError,
    Result,
};
use operator::{OperatorPending, OperatorRunning};

pub struct LiveProcess {
    config: LiveProcessConfig,
    shutdown_tx: broadcast::Sender<()>,
    sync_controller: Arc<SyncController>,
    operator_running: OperatorRunning,
    executor_updates_handle: AbortOnDropHandle<()>,
    trade_executor: Arc<LiveTradeExecutor>,
    status_manager: Arc<LiveStatusManager>,
    update_tx: LiveTransmiter,
}

impl LiveProcess {
    pub fn spawn(
        config: &LiveConfig,
        shutdown_tx: broadcast::Sender<()>,
        sync_engine: SyncEngine,
        operator_pending: OperatorPending,
        trade_executor_launcher: LiveTradeExecutorLauncher,
        status_manager: Arc<LiveStatusManager>,
        update_tx: LiveTransmiter,
    ) -> AbortOnDropHandle<LiveProcessFatalResult<()>> {
        let config = config.into();

        tokio::spawn(async move {
            let sync_controller = sync_engine.start();

            let executor_rx = trade_executor_launcher.update_receiver();

            let executor_updates_handle = Self::spawn_executor_update_handler(
                status_manager.clone(),
                update_tx.clone(),
                executor_rx,
            );

            let trade_executor = match trade_executor_launcher.launch().await {
                Ok(tex) => tex,
                Err(e) => {
                    status_manager.update(LiveProcessFatalError::LaunchExecutor(e).into());
                    return Ok(());
                }
            };

            let operator_running = match operator_pending.start(trade_executor.clone()).await {
                Ok(op) => op,
                Err(e) => {
                    status_manager.update(e.into());
                    return Ok(());
                }
            };

            let process = Self {
                config,
                shutdown_tx,
                sync_controller,
                operator_running,
                executor_updates_handle,
                trade_executor,
                status_manager,
                update_tx,
            };

            process.recovery_loop().await
        })
        .into()
    }

    fn spawn_executor_update_handler(
        status_manager: Arc<LiveStatusManager>,
        update_tx: LiveTransmiter,
        mut executor_rx: LiveTradeExecutorReceiver,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                match executor_rx.recv().await {
                    Ok(executor_update) => match executor_update {
                        LiveTradeExecutorUpdate::Status(_) => {} // Handled in `run_operator`
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
                        let e = LiveProcessRecoverableError::ExecutorRecvLagged { skipped };
                        status_manager.update(e.into());
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

    // Only possibily returns errors if they took place during shutdown.
    // Other `LiveProcessFatalError`s will result in `Ok` and should be accessed
    // via `LiveStatus`.
    async fn recovery_loop(self) -> LiveProcessFatalResult<()> {
        loop {
            self.status_manager.update(LiveStatus::Starting);

            let mut shutdown_rx = self.shutdown_tx.subscribe();

            let live_process_error = tokio::select! {
                Err(e) = self.run_operator() => e,
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
                        // Shutdown signal received

                        return self.shutdown().await;
                    };

                    LiveProcessFatalError::ShutdownSignalRecv(e).into()
                }
            };

            match live_process_error {
                LiveProcessError::Fatal(e) => {
                    self.status_manager.update(e.into());
                    return Ok(());
                }
                LiveProcessError::Recoverable(e) => {
                    self.status_manager.update(e.into());
                }
            }

            self.status_manager.update(LiveStatus::Restarting);

            // Handle shutdown signals while waiting for `restart_interval`

            tokio::select! {
                _ = time::sleep(self.config.restart_interval()) => {} // Loop restarts
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
                        // Shutdown signal received

                        return self.shutdown().await;
                    };

                    let status = LiveProcessFatalError::ShutdownSignalRecv(e).into();
                    self.status_manager.update(status);

                    return Ok(());
                }
            }
        }
    }

    fn run_operator(&self) -> Pin<Box<dyn Future<Output = Result<Never>> + Send + '_>> {
        match &self.operator_running {
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
                .map_err(LiveProcessRecoverableError::OperatorError)?;

            let now = {
                let target_exec = (last_eval + iteration_interval).ceil_sec();
                let now = Utc::now();

                if now >= target_exec {
                    return Err(LiveProcessRecoverableError::OperatorIterationTimeTooLong.into());
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
                                            return Err(LiveProcessFatalError::SyncProcessTerminated(err).into());
                                        }
                                        SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                            return Err(LiveProcessFatalError::SyncProcessShutdown.into());
                                        }
                                    },
                                    SyncUpdate::PriceTick(_) => break,
                                    SyncUpdate::PriceHistoryState(_) => {
                                        // TODO: Improve feedback on price history updates
                                        // Sync may take a long time when `sync_mode_full: true`
                                    }
                                },
                                Err(RecvError::Lagged(skipped)) => return Err(LiveProcessRecoverableError::SyncRecvLagged{skipped}.into()),
                                Err(RecvError::Closed) => return Err(LiveProcessFatalError::SyncRecvClosed.into())
                            }
                        }
                        _ = time::sleep(self.config.sync_update_timeout()) => {
                            if matches!(sync_reader.status_snapshot(), SyncStatus::Synced) {
                                break;
                            }
                        }
                    }
                }

                last_eval = Utc::now();
                continue;
            }

            let tex_state = self.trade_executor.state_snapshot().await;
            let tex_status = tex_state.status();

            match tex_status {
                LiveTradeExecutorStatus::Ready => {
                    self.status_manager
                        .update_if_not_running(LiveStatus::Running);
                }
                LiveTradeExecutorStatus::NotReady(tex_status_not_ready) => {
                    self.status_manager.update(LiveStatus::WaitingTradeExecutor(
                        tex_status_not_ready.clone(),
                    ));
                    continue;
                }
            }

            let ctx_window = raw_operator
                .context_window_secs()
                .map_err(LiveProcessRecoverableError::OperatorError)?;

            let ctx_entries = db
                .price_ticks
                .compute_locf_entries_for_range(now, ctx_window)
                .await
                .map_err(LiveProcessRecoverableError::Db)?;

            raw_operator
                .iterate(ctx_entries.as_slice())
                .await
                .map_err(LiveProcessRecoverableError::OperatorError)?;
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
                            return Err(
                                LiveProcessFatalError::LiveSignalProcessTerminated(err).into()
                            );
                        }
                        LiveSignalStatus::ShutdownInitiated | LiveSignalStatus::Shutdown => {
                            return Err(LiveProcessFatalError::LiveSignalProcessShutdown.into());
                        }
                    },
                    LiveSignalUpdate::Signal(new_signal) => {
                        let tex_state = self.trade_executor.state_snapshot().await;
                        let tex_status = tex_state.status();

                        match tex_status {
                            LiveTradeExecutorStatus::Ready => {
                                self.status_manager
                                    .update_if_not_running(LiveStatus::Running);
                            }
                            LiveTradeExecutorStatus::NotReady(tex_status_not_ready) => {
                                self.status_manager.update(LiveStatus::WaitingTradeExecutor(
                                    tex_status_not_ready.clone(),
                                ));
                                continue;
                            }
                        }

                        // Send Signal update
                        let _ = self.update_tx.send(new_signal.clone().into());

                        signal_operator
                            .process_signal(&new_signal)
                            .await
                            .map_err(LiveProcessRecoverableError::OperatorError)?;
                    }
                },
                Err(RecvError::Lagged(skipped)) => {
                    return Err(LiveProcessRecoverableError::SignalRecvLagged { skipped }.into());
                }
                Err(RecvError::Closed) => {
                    return Err(LiveProcessFatalError::SignalRecvClosed.into());
                }
            }
        }
    }

    async fn shutdown(self) -> LiveProcessFatalResult<()> {
        self.executor_updates_handle.abort();

        let executor_shutdown_res = self
            .trade_executor
            .shutdown()
            .await
            .map_err(|e| LiveProcessFatalError::ExecutorShutdownError(e).into());

        let signal_shutdown_res = match self.operator_running.signal_controller() {
            Some(signal_controller) => signal_controller.shutdown().await,
            None => Ok(()),
        }
        .map_err(|e| LiveProcessFatalError::LiveSignalShutdown(e).into());

        let sync_shutdown_res = self
            .sync_controller
            .shutdown()
            .await
            .map_err(|e| LiveProcessFatalError::SyncShutdown(e).into());

        executor_shutdown_res
            .and(signal_shutdown_res)
            .and(sync_shutdown_res)
    }
}
