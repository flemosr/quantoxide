use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    db::Database,
    shared::Lookback,
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::{
    config::{LiveSignalConfig, LiveSignalProcessConfig},
    core::{ConfiguredSignalEvaluator, Signal},
    state::{LiveSignalStatusManager, LiveSignalStatusNotRunning, LiveSignalTransmiter},
};

pub(crate) mod error;

use error::{
    ProcessResult, SignalProcessError, SignalProcessFatalError, SignalProcessRecoverableError,
};

pub(super) struct LiveSignalProcess {
    config: LiveSignalProcessConfig,
    db: Arc<Database>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    shutdown_tx: broadcast::Sender<()>,
    sync_reader: Arc<dyn SyncReader>,
    status_manager: Arc<LiveSignalStatusManager>,
    update_tx: LiveSignalTransmiter,
}

impl LiveSignalProcess {
    pub fn spawn(
        config: &LiveSignalConfig,
        db: Arc<Database>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
        shutdown_tx: broadcast::Sender<()>,
        sync_reader: Arc<dyn SyncReader>,
        status_manager: Arc<LiveSignalStatusManager>,
        update_tx: LiveSignalTransmiter,
    ) -> AbortOnDropHandle<()> {
        let config = config.into();

        tokio::spawn(async move {
            let process = Self {
                config,
                db,
                evaluators,
                shutdown_tx,
                sync_reader,
                status_manager,
                update_tx,
            };

            process.recovery_loop().await
        })
        .into()
    }

    async fn run(&self) -> ProcessResult<Never> {
        let mut min_iteration_interval = Duration::MAX;
        let mut max_lookback: Option<Lookback> = None;
        let mut evaluators = Vec::with_capacity(self.evaluators.len());

        let now = Utc::now().ceil_sec();
        for evaluator in self.evaluators.iter() {
            min_iteration_interval =
                min_iteration_interval.min(evaluator.min_iteration_interval().as_duration());

            if let Some(lookback) = evaluator.lookback() {
                max_lookback = Some(match max_lookback {
                    Some(existing) if existing.period() >= lookback.period() => existing,
                    _ => lookback,
                });
            }

            evaluators.push((now, evaluator));
        }

        let mut next_eval = now + min_iteration_interval;

        loop {
            let mut now = Utc::now();
            if now < next_eval {
                let wait_duration = (next_eval - now).to_std().expect("valid duration");
                time::sleep(wait_duration).await;
                now = next_eval;
            }

            if !matches!(self.sync_reader.status_snapshot(), SyncStatus::Synced) {
                let mut sync_rx = self.sync_reader.update_receiver();
                loop {
                    tokio::select! {
                        sync_update_result = sync_rx.recv() => {
                            match sync_update_result {
                                Ok(sync_update) => {
                                    match sync_update {
                                        SyncUpdate::Status(sync_status) => match sync_status {
                                            SyncStatus::NotSynced(sync_status_not_synced) => {
                                                self.status_manager.update(
                                                    LiveSignalStatusNotRunning::WaitingForSync(
                                                        sync_status_not_synced,
                                                    )
                                                    .into(),
                                                )
                                            }
                                            SyncStatus::Synced => break,
                                            SyncStatus::Terminated(err) => {
                                                // Non-recoverable error
                                                return Err(SignalProcessFatalError::SyncProcessTerminated(err).into());
                                            }
                                            SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                                // Non-recoverable error
                                                return Err(SignalProcessFatalError::SyncProcessShutdown.into());
                                            }
                                        },
                                        SyncUpdate::PriceTick(_) => break,
                                        SyncUpdate::PriceHistoryState(_) => {}
                                    }
                                },
                                Err(RecvError::Lagged(skipped)) => {
                                    return Err(SignalProcessRecoverableError::SyncRecvLagged { skipped }.into());
                                },
                                Err(RecvError::Closed) => {
                                    return Err(SignalProcessFatalError::SyncRecvClosed.into());
                                }
                            }
                        }
                        _ = time::sleep(self.config.sync_update_timeout()) => {
                            if matches!(self.sync_reader.status_snapshot(), SyncStatus::Synced) {
                                break;
                            }
                        }
                    }
                }
            }

            let candle_buffer = if let Some(lookback) = max_lookback {
                // Floor current time to the resolution boundary to get the current, possibly
                // incomplete, candle.
                let now = Utc::now();
                let resolution = lookback.resolution();
                let current_bucket = now.floor_to_resolution(resolution);
                let from =
                    current_bucket.step_back_candles(resolution, lookback.period().as_u64() - 1);

                self.db
                    .ohlc_candles
                    .get_candles_consolidated(from, now, resolution)
                    .await
                    .map_err(SignalProcessRecoverableError::Db)?
            } else {
                Vec::new()
            };

            next_eval = DateTime::<Utc>::MAX_UTC;

            for (last_eval, evaluator) in evaluators.iter_mut() {
                if now < *last_eval + evaluator.min_iteration_interval().as_duration() {
                    continue;
                }

                *last_eval = now;

                let evaluator_next_eval = now + evaluator.min_iteration_interval().as_duration();
                if evaluator_next_eval < next_eval {
                    next_eval = evaluator_next_eval;
                }

                let start_idx = candle_buffer
                    .len()
                    .saturating_sub(evaluator.lookback().map_or(0, |l| l.period().as_usize()));
                let candles = &candle_buffer[start_idx..];

                let signal = Signal::try_evaluate(evaluator, now, candles).await?;

                let _ = self.update_tx.send(signal.into());
            }
        }
    }

    async fn recovery_loop(self) {
        self.status_manager
            .update(LiveSignalStatusNotRunning::Starting.into());

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            let signal_process_error = tokio::select! {
                Err(signal_error) = self.run() => signal_error,
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
                       // Shutdown signal received
                       return;
                    };

                    SignalProcessFatalError::ShutdownSignalRecv(e).into()
                }
            };

            match signal_process_error {
                SignalProcessError::Fatal(err) => {
                    self.status_manager.update(err.into());
                    return;
                }
                SignalProcessError::Recoverable(err) => {
                    self.status_manager.update(err.into());
                }
            }

            // Handle shutdown signals while waiting for `restart_interval`

            tokio::select! {
                _ = time::sleep(self.config.restart_interval()) => {} // Loop restarts
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        let status = SignalProcessFatalError::ShutdownSignalRecv(e).into();
                        self.status_manager.update(status);
                    }
                    return;
                }
            }

            self.status_manager
                .update(LiveSignalStatusNotRunning::Restarting.into());
        }
    }
}
