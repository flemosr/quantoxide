use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    db::{Database, models::OhlcCandleRow},
    shared::{OhlcResolution, Period},
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::{
    config::{LiveSignalConfig, LiveSignalProcessConfig},
    core::{Signal, WrappedSignalEvaluator},
    state::{
        LiveSignalStatusManager, LiveSignalStatusNotRunning, LiveSignalTransmitter,
        LiveSignalUpdate,
    },
};

pub(crate) mod error;

use error::{
    ProcessResult, SignalProcessError, SignalProcessFatalError, SignalProcessRecoverableError,
};

/// Groups evaluators that share the same OHLC resolution.
struct ResolutionGroup {
    max_period: Period,
    /// (last_eval_time, evaluator_index, period)
    evaluators: Vec<(DateTime<Utc>, usize, Period)>,
}

impl ResolutionGroup {
    fn new(initial_period: Period) -> Self {
        Self {
            max_period: initial_period,
            evaluators: Vec::new(),
        }
    }
}

pub(super) struct LiveSignalProcess<S: Signal> {
    config: LiveSignalProcessConfig,
    db: Arc<Database>,
    evaluators: Vec<WrappedSignalEvaluator<S>>,
    shutdown_tx: broadcast::Sender<()>,
    sync_reader: Arc<dyn SyncReader>,
    status_manager: Arc<LiveSignalStatusManager<S>>,
    update_tx: LiveSignalTransmitter<S>,
}

impl<S: Signal> LiveSignalProcess<S> {
    pub fn spawn(
        config: &LiveSignalConfig,
        db: Arc<Database>,
        evaluators: Vec<WrappedSignalEvaluator<S>>,
        shutdown_tx: broadcast::Sender<()>,
        sync_reader: Arc<dyn SyncReader>,
        status_manager: Arc<LiveSignalStatusManager<S>>,
        update_tx: LiveSignalTransmitter<S>,
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

        let mut resolution_groups: HashMap<OhlcResolution, ResolutionGroup> = HashMap::new();
        let mut no_lookback_evaluators: Vec<(DateTime<Utc>, usize)> = Vec::new();

        let now = Utc::now().ceil_sec();
        for (idx, evaluator) in self.evaluators.iter().enumerate() {
            min_iteration_interval = min_iteration_interval.min(
                evaluator
                    .min_iteration_interval()
                    .map_err(SignalProcessFatalError::Evaluator)?
                    .as_duration(),
            );

            match evaluator
                .lookback()
                .map_err(SignalProcessFatalError::Evaluator)?
            {
                Some(lookback) => {
                    let group = resolution_groups
                        .entry(lookback.resolution())
                        .or_insert_with(|| ResolutionGroup::new(lookback.period()));

                    if lookback.period() > group.max_period {
                        group.max_period = lookback.period();
                    }

                    group.evaluators.push((now, idx, lookback.period()));
                }
                None => {
                    no_lookback_evaluators.push((now, idx));
                }
            }
        }

        let mut next_eval = now + min_iteration_interval;

        loop {
            if Utc::now() < next_eval {
                let wait_duration = (next_eval - Utc::now()).to_std().expect("valid duration");
                time::sleep(wait_duration).await;
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
                                        SyncUpdate::FundingSettlementsState(_) => {}
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

            let now = Utc::now();
            let mut candle_buffers: HashMap<OhlcResolution, Vec<OhlcCandleRow>> = HashMap::new();

            for (resolution, group) in &resolution_groups {
                let current_bucket = now.floor_to_resolution(*resolution);
                let from =
                    current_bucket.step_back_candles(*resolution, group.max_period.as_u64() - 1);

                let candles = self
                    .db
                    .ohlc_candles
                    .get_candles_consolidated(from, now, *resolution)
                    .await
                    .map_err(SignalProcessRecoverableError::Db)?;

                candle_buffers.insert(*resolution, candles);
            }

            next_eval = DateTime::<Utc>::MAX_UTC;

            for (resolution, group) in resolution_groups.iter_mut() {
                let candle_buffer = candle_buffers
                    .get(resolution)
                    .map(|v| v.as_slice())
                    .expect("resolution must be available");

                for (last_eval, evaluator_idx, period) in group.evaluators.iter_mut() {
                    let evaluator = &self.evaluators[*evaluator_idx];

                    let eval_interval = evaluator
                        .min_iteration_interval()
                        .map_err(SignalProcessFatalError::Evaluator)?
                        .as_duration();

                    if now < *last_eval + eval_interval {
                        continue;
                    }

                    *last_eval = now;

                    let evaluator_next_eval = now + eval_interval;
                    if evaluator_next_eval < next_eval {
                        next_eval = evaluator_next_eval;
                    }

                    let start_idx = candle_buffer.len().saturating_sub(period.as_usize());
                    let candles = &candle_buffer[start_idx..];

                    let signal = evaluator
                        .evaluate(candles)
                        .await
                        .map_err(SignalProcessRecoverableError::Evaluator)?;

                    let _ = self.update_tx.send(LiveSignalUpdate::Signal(signal));
                }
            }

            for (last_eval, evaluator_idx) in no_lookback_evaluators.iter_mut() {
                let evaluator = &self.evaluators[*evaluator_idx];

                let eval_interval = evaluator
                    .min_iteration_interval()
                    .map_err(SignalProcessFatalError::Evaluator)?
                    .as_duration();

                if now < *last_eval + eval_interval {
                    continue;
                }

                *last_eval = now;

                let evaluator_next_eval = now + eval_interval;
                if evaluator_next_eval < next_eval {
                    next_eval = evaluator_next_eval;
                }

                let signal = evaluator
                    .evaluate(&[])
                    .await
                    .map_err(SignalProcessRecoverableError::Evaluator)?;

                let _ = self.update_tx.send(LiveSignalUpdate::Signal(signal));
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
