use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::{sync::broadcast::error::RecvError, time};

use crate::{
    db::{Database, models::OhlcCandleRow},
    signal::{LiveSignalController, LiveSignalEngine, LiveSignalStatus, LiveSignalUpdate, Signal},
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{DateTimeExt, Never},
};

use super::{
    super::{
        super::core::{TradeExecutor, WrappedRawOperator, WrappedSignalOperator},
        config::LiveProcessConfig,
        executor::{
            LiveTradeExecutor,
            state::{LiveTradeExecutorStatus, LiveTradeExecutorStatusNotReady},
        },
        state::{LiveTradeStatus, LiveTradeStatusManager, LiveTradeTransmitter, LiveTradeUpdate},
    },
    error::{LiveProcessFatalError, LiveProcessFatalResult, LiveProcessRecoverableError, Result},
};

/// Pending operator state before starting.
pub(in crate::trade::live) enum OperatorPending<S: Signal> {
    Signal {
        signal_engine: LiveSignalEngine<S>,
        signal_operator: WrappedSignalOperator<S>,
    },
    Raw {
        db: Arc<Database>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl<S: Signal> OperatorPending<S> {
    pub fn signal(
        signal_engine: LiveSignalEngine<S>,
        signal_operator: WrappedSignalOperator<S>,
    ) -> Self {
        Self::Signal {
            signal_engine,
            signal_operator,
        }
    }

    pub fn raw(
        db: Arc<Database>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    ) -> Self {
        Self::Raw {
            db,
            sync_reader,
            raw_operator,
        }
    }

    pub fn start(
        self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> LiveProcessFatalResult<OperatorRunning<S>> {
        match self {
            OperatorPending::Signal {
                signal_engine,
                mut signal_operator,
            } => {
                signal_operator
                    .set_trade_executor(trade_executor)
                    .map_err(LiveProcessFatalError::StartOperatorError)?;

                let signal_controller = signal_engine.start();

                Ok(OperatorRunning::Signal {
                    signal_controller,
                    signal_operator,
                })
            }
            OperatorPending::Raw {
                db,
                sync_reader,
                mut raw_operator,
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(LiveProcessFatalError::StartOperatorError)?;

                Ok(OperatorRunning::Raw {
                    db,
                    sync_reader,
                    raw_operator,
                })
            }
        }
    }
}

/// Running operator state.
pub(in crate::trade::live) enum OperatorRunning<S: Signal> {
    Signal {
        signal_controller: Arc<LiveSignalController<S>>,
        signal_operator: WrappedSignalOperator<S>,
    },
    Raw {
        db: Arc<Database>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl<S: Signal> OperatorRunning<S> {
    pub async fn run(
        &self,
        config: &LiveProcessConfig,
        trade_executor: &Arc<LiveTradeExecutor>,
        status_manager: &Arc<LiveTradeStatusManager<S>>,
        update_tx: &LiveTradeTransmitter<S>,
    ) -> Result<Never> {
        match self {
            OperatorRunning::Signal {
                signal_controller,
                signal_operator,
            } => {
                Self::run_signal(
                    signal_controller,
                    signal_operator,
                    trade_executor,
                    status_manager,
                    update_tx,
                )
                .await
            }
            OperatorRunning::Raw {
                db,
                sync_reader,
                raw_operator,
            } => {
                Self::run_raw(
                    db,
                    sync_reader,
                    raw_operator,
                    config,
                    trade_executor,
                    status_manager,
                )
                .await
            }
        }
    }

    pub async fn shutdown(&self) -> LiveProcessFatalResult<()> {
        match self {
            OperatorRunning::Signal {
                signal_controller, ..
            } => signal_controller
                .shutdown()
                .await
                .map_err(LiveProcessFatalError::LiveSignalShutdown),
            OperatorRunning::Raw { .. } => Ok(()),
        }
    }

    async fn run_signal(
        signal_controller: &Arc<LiveSignalController<S>>,
        signal_operator: &WrappedSignalOperator<S>,
        trade_executor: &Arc<LiveTradeExecutor>,
        status_manager: &Arc<LiveTradeStatusManager<S>>,
        update_tx: &LiveTradeTransmitter<S>,
    ) -> Result<Never> {
        loop {
            match signal_controller.update_receiver().recv().await {
                Ok(signal_update) => match signal_update {
                    LiveSignalUpdate::Status(signal_status) => match signal_status {
                        LiveSignalStatus::NotRunning(signal_status_not_running) => {
                            status_manager.update(LiveTradeStatus::WaitingForSignal(
                                signal_status_not_running,
                            ));
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
                        let tex_state = trade_executor.state_snapshot().await;
                        let tex_status = tex_state.status();

                        match tex_status {
                            LiveTradeExecutorStatus::Ready => {
                                status_manager.update_if_not_running(LiveTradeStatus::Running);
                            }
                            LiveTradeExecutorStatus::NotReady(tex_status_not_ready) => {
                                match tex_status_not_ready {
                                    LiveTradeExecutorStatusNotReady::Terminated(e) => {
                                        return Err(
                                            LiveProcessFatalError::ExecutorProcessTerminated(
                                                e.clone(),
                                            )
                                            .into(),
                                        );
                                    }
                                    LiveTradeExecutorStatusNotReady::ShutdownInitiated
                                    | LiveTradeExecutorStatusNotReady::Shutdown => {
                                        return Err(
                                            LiveProcessFatalError::ExecutorProcessShutdown.into()
                                        );
                                    }
                                    LiveTradeExecutorStatusNotReady::Starting
                                    | LiveTradeExecutorStatusNotReady::WaitingForSync(_)
                                    | LiveTradeExecutorStatusNotReady::Failed(_) => {
                                        status_manager.update(
                                            LiveTradeStatus::WaitingTradeExecutor(
                                                tex_status_not_ready.clone(),
                                            ),
                                        );
                                        continue;
                                    }
                                }
                            }
                        }

                        let _ = update_tx.send(LiveTradeUpdate::Signal(new_signal.clone()));

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

    async fn run_raw(
        db: &Arc<Database>,
        sync_reader: &Arc<dyn SyncReader>,
        raw_operator: &WrappedRawOperator,
        config: &LiveProcessConfig,
        trade_executor: &Arc<LiveTradeExecutor>,
        status_manager: &Arc<LiveTradeStatusManager<S>>,
    ) -> Result<Never> {
        let mut last_eval = Utc::now();

        loop {
            let min_iteration_interval = raw_operator
                .min_iteration_interval()
                .map_err(LiveProcessRecoverableError::OperatorError)?
                .as_duration();

            let target_exec = (last_eval + min_iteration_interval).ceil_sec();
            let now = Utc::now();

            if now < target_exec {
                let wait_duration = (target_exec - now).to_std().expect("valid duration");
                time::sleep(wait_duration).await;
            }

            if let SyncStatus::NotSynced(sync_status_not_synced) = sync_reader.status_snapshot() {
                status_manager.update(LiveTradeStatus::WaitingForSync(sync_status_not_synced));
                Self::wait_for_sync(sync_reader, config, status_manager).await?;

                continue;
            }

            last_eval = Utc::now();

            let tex_state = trade_executor.state_snapshot().await;
            let tex_status = tex_state.status();

            match tex_status {
                LiveTradeExecutorStatus::Ready => {
                    status_manager.update_if_not_running(LiveTradeStatus::Running);
                }
                LiveTradeExecutorStatus::NotReady(tex_status_not_ready) => {
                    match tex_status_not_ready {
                        LiveTradeExecutorStatusNotReady::Terminated(e) => {
                            return Err(LiveProcessFatalError::ExecutorProcessTerminated(
                                e.clone(),
                            )
                            .into());
                        }
                        LiveTradeExecutorStatusNotReady::ShutdownInitiated
                        | LiveTradeExecutorStatusNotReady::Shutdown => {
                            return Err(LiveProcessFatalError::ExecutorProcessShutdown.into());
                        }
                        LiveTradeExecutorStatusNotReady::Starting
                        | LiveTradeExecutorStatusNotReady::WaitingForSync(_)
                        | LiveTradeExecutorStatusNotReady::Failed(_) => {
                            status_manager.update(LiveTradeStatus::WaitingTradeExecutor(
                                tex_status_not_ready.clone(),
                            ));
                            continue;
                        }
                    }
                }
            }

            let candles = Self::fetch_candles(db, raw_operator, last_eval).await?;

            raw_operator
                .iterate(candles.as_slice())
                .await
                .map_err(LiveProcessRecoverableError::OperatorError)?;
        }
    }

    async fn wait_for_sync(
        sync_reader: &Arc<dyn SyncReader>,
        config: &LiveProcessConfig,
        status_manager: &Arc<LiveTradeStatusManager<S>>,
    ) -> Result<()> {
        let mut sync_rx = sync_reader.update_receiver();
        loop {
            tokio::select! {
                sync_update_result = sync_rx.recv() => {
                    match sync_update_result {
                        Ok(sync_update) => match sync_update {
                            SyncUpdate::Status(sync_status) => match sync_status {
                                SyncStatus::NotSynced(sync_status_not_synced) => {
                                    status_manager.update(
                                        LiveTradeStatus::WaitingForSync(sync_status_not_synced)
                                    );
                                }
                                SyncStatus::Synced => return Ok(()),
                                SyncStatus::Terminated(err) => {
                                    return Err(LiveProcessFatalError::SyncProcessTerminated(err).into());
                                }
                                SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                    return Err(LiveProcessFatalError::SyncProcessShutdown.into());
                                }
                            },
                            SyncUpdate::PriceTick(_) => return Ok(()),
                            SyncUpdate::PriceHistoryState(_) => {}
                        },
                        Err(RecvError::Lagged(skipped)) => {
                            return Err(LiveProcessRecoverableError::SyncRecvLagged { skipped }.into());
                        },
                        Err(RecvError::Closed) => {
                            return Err(LiveProcessFatalError::SyncRecvClosed.into());
                        }
                    }
                }
                _ = time::sleep(config.sync_update_timeout()) => {
                    if matches!(sync_reader.status_snapshot(), SyncStatus::Synced) {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn fetch_candles(
        db: &Arc<Database>,
        raw_operator: &WrappedRawOperator,
        now: DateTime<Utc>,
    ) -> Result<Vec<OhlcCandleRow>> {
        let lookback = raw_operator
            .lookback()
            .map_err(LiveProcessRecoverableError::OperatorError)?;

        if let Some(lookback) = lookback {
            let resolution = lookback.resolution();
            let current_bucket = now.floor_to_resolution(resolution);
            let from = current_bucket.step_back_candles(resolution, lookback.period().as_u64() - 1);

            db.ohlc_candles
                .get_candles_consolidated(from, now, resolution)
                .await
                .map_err(LiveProcessRecoverableError::Db)
                .map_err(Into::into)
        } else {
            Ok(Vec::new())
        }
    }
}
