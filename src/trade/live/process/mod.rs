use std::sync::Arc;

use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    signal::Signal,
    sync::{SyncController, SyncEngine},
    util::AbortOnDropHandle,
};

use super::{
    config::{LiveProcessConfig, LiveTradeConfig},
    executor::{
        LiveTradeExecutor, LiveTradeExecutorLauncher,
        update::{LiveTradeExecutorReceiver, LiveTradeExecutorUpdate},
    },
    state::{LiveTradeStatus, LiveTradeStatusManager, LiveTradeTransmitter, LiveTradeUpdate},
};

pub(crate) mod error;
pub(in crate::trade) mod operator;

use error::{
    LiveProcessError, LiveProcessFatalError, LiveProcessFatalResult, LiveProcessRecoverableError,
};

use operator::{OperatorPending, OperatorRunning};

pub(super) struct LiveProcess<S: Signal> {
    config: LiveProcessConfig,
    shutdown_tx: broadcast::Sender<()>,
    sync_controller: Arc<SyncController>,
    operator_running: OperatorRunning<S>,
    executor_updates_handle: AbortOnDropHandle<()>,
    trade_executor: Arc<LiveTradeExecutor>,
    status_manager: Arc<LiveTradeStatusManager<S>>,
    update_tx: LiveTradeTransmitter<S>,
}

impl<S: Signal> LiveProcess<S> {
    pub fn spawn(
        config: &LiveTradeConfig,
        shutdown_tx: broadcast::Sender<()>,
        sync_engine: SyncEngine,
        operator_pending: OperatorPending<S>,
        trade_executor_launcher: LiveTradeExecutorLauncher,
        status_manager: Arc<LiveTradeStatusManager<S>>,
    ) -> AbortOnDropHandle<LiveProcessFatalResult<()>> {
        let config = config.into();

        tokio::spawn(async move {
            let sync_controller = sync_engine.start();

            let executor_rx = trade_executor_launcher.update_receiver();

            let update_tx = status_manager.transmitter().clone();

            let executor_updates_handle = Self::spawn_executor_update_handler(
                status_manager.clone(),
                update_tx.clone(),
                executor_rx,
            );

            let trade_executor = match trade_executor_launcher
                .launch()
                .await
                .map_err(LiveProcessFatalError::LaunchExecutor)
            {
                Ok(tex) => tex,
                Err(e) => {
                    status_manager.update(e.into());
                    return Ok(());
                }
            };

            let operator_running = match operator_pending.start(trade_executor.clone()) {
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
        status_manager: Arc<LiveTradeStatusManager<S>>,
        update_tx: LiveTradeTransmitter<S>,
        mut executor_rx: LiveTradeExecutorReceiver,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                match executor_rx.recv().await {
                    Ok(executor_update) => match executor_update {
                        LiveTradeExecutorUpdate::Status(_) => {} // Handled in operator runner
                        LiveTradeExecutorUpdate::Order(executor_update_order) => {
                            let _ = update_tx.send(executor_update_order.into());
                        }
                        LiveTradeExecutorUpdate::TradingState(trading_state) => {
                            let _ = update_tx.send(trading_state.into());
                        }
                        LiveTradeExecutorUpdate::ClosedTrade(closed_trade) => {
                            let _ = update_tx.send(LiveTradeUpdate::ClosedTrade(closed_trade));
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

    async fn recovery_loop(self) -> LiveProcessFatalResult<()> {
        self.status_manager.update(LiveTradeStatus::Starting);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            let live_process_error = tokio::select! {
                Err(e) = self.operator_running.run(
                    &self.config,
                    &self.trade_executor,
                    &self.status_manager,
                    &self.update_tx,
                ) => e,
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
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

            tokio::select! {
                _ = time::sleep(self.config.restart_interval()) => {}
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
                        return self.shutdown().await;
                    };

                    let status = LiveProcessFatalError::ShutdownSignalRecv(e).into();
                    self.status_manager.update(status);

                    return Ok(());
                }
            }

            self.status_manager.update(LiveTradeStatus::Restarting);
        }
    }

    async fn shutdown(self) -> LiveProcessFatalResult<()> {
        self.executor_updates_handle.abort();

        let executor_shutdown_res = self
            .trade_executor
            .shutdown()
            .await
            .map_err(LiveProcessFatalError::ExecutorShutdownError);

        let operator_shutdown_res = self.operator_running.shutdown().await;

        let sync_shutdown_res = self
            .sync_controller
            .shutdown()
            .await
            .map_err(LiveProcessFatalError::SyncShutdown);

        executor_shutdown_res
            .and(operator_shutdown_res)
            .and(sync_shutdown_res)
    }
}
