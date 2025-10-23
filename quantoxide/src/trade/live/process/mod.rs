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
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::{
    super::core::{WrappedRawOperator, WrappedSignalOperator},
    engine::LiveConfig,
    executor::{LiveTradeExecutor, state::LiveTradeExecutorStatus},
    state::{LiveStatus, LiveStatusManager, LiveTransmiter},
};

pub mod error;

use error::{LiveProcessError, Result};

pub enum OperatorRunning {
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
    pub fn signal_controller(&self) -> Option<Arc<LiveSignalController>> {
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
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
        }
    }
}

pub struct LiveProcess {
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
                .map_err(LiveProcessError::OperatorError)?;

            let now = {
                let target_exec = (last_eval + iteration_interval).ceil_sec();
                let now = Utc::now();

                if now >= target_exec {
                    return Err(LiveProcessError::OperatorIterationTimeTooLong);
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
                                            return Err(LiveProcessError::SyncProcessTerminated(err));
                                        }
                                        SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                            return Err(LiveProcessError::SyncProcessShutdown);
                                        }
                                    },
                                    SyncUpdate::PriceTick(_) => break,
                                    SyncUpdate::PriceHistoryState(_) => {
                                        // TODO: Improve feedback on price history updates
                                        // Sync may take a long time when `sync_mode_full: true`
                                    }
                                },
                                Err(RecvError::Lagged(skipped)) => return Err(LiveProcessError::SyncRecvLagged{skipped}.into()),
                                Err(RecvError::Closed) => return Err(LiveProcessError::SyncRecvClosed.into())
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
                .map_err(LiveProcessError::OperatorError)?;

            let ctx_entries = db
                .price_ticks
                .compute_locf_entries_for_range(now, ctx_window)
                .await?;

            raw_operator
                .iterate(ctx_entries.as_slice())
                .await
                .map_err(LiveProcessError::OperatorError)?;
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
                            return Err(LiveProcessError::LiveSignalProcessTerminated(err));
                        }
                        LiveSignalStatus::ShutdownInitiated | LiveSignalStatus::Shutdown => {
                            return Err(LiveProcessError::LiveSignalProcessShutdown);
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
                            .map_err(LiveProcessError::OperatorError)?;
                    }
                },
                Err(RecvError::Lagged(skipped)) => {
                    return Err(LiveProcessError::SignalRecvLagged { skipped }.into());
                }
                Err(RecvError::Closed) => return Err(LiveProcessError::SignalRecvClosed.into()),
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
                                LiveProcessError::ShutdownSignalRecv(e))
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
                                LiveProcessError::ShutdownSignalRecv(e))
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
