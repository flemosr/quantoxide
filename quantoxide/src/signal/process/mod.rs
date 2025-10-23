use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    db::DbContext,
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, DateTimeExt, Never},
};

use super::{
    core::{ConfiguredSignalEvaluator, Signal},
    engine::LiveSignalConfig,
    state::{LiveSignalStatusManager, LiveSignalStatusNotRunning, LiveSignalTransmiter},
};

pub mod error;

use error::{ProcessResult, SignalProcessError};

#[derive(Clone, Debug)]
pub struct LiveSignalProcessConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
}

impl From<&LiveSignalConfig> for LiveSignalProcessConfig {
    fn from(value: &LiveSignalConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
        }
    }
}

pub struct LiveSignalProcess {
    config: LiveSignalProcessConfig,
    db: Arc<DbContext>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    shutdown_tx: broadcast::Sender<()>,
    sync_reader: Arc<dyn SyncReader>,
    status_manager: Arc<LiveSignalStatusManager>,
    update_tx: LiveSignalTransmiter,
}

impl LiveSignalProcess {
    pub fn new(
        config: &LiveSignalConfig,
        db: Arc<DbContext>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
        shutdown_tx: broadcast::Sender<()>,
        sync_reader: Arc<dyn SyncReader>,
        status_manager: Arc<LiveSignalStatusManager>,
        update_tx: LiveSignalTransmiter,
    ) -> Self {
        Self {
            config: config.into(),
            db,
            evaluators,
            shutdown_tx,
            sync_reader,
            status_manager,
            update_tx,
        }
    }

    async fn run(&self) -> ProcessResult<Never> {
        let mut min_evaluation_interval = Duration::MAX;
        let mut max_ctx_window = usize::MIN;
        let mut evaluators = Vec::with_capacity(self.evaluators.len());

        let now = Utc::now().ceil_sec();
        for evaluator in self.evaluators.iter() {
            if evaluator.evaluation_interval() < min_evaluation_interval {
                min_evaluation_interval = evaluator.evaluation_interval();
            }
            if evaluator.context_window_secs() > max_ctx_window {
                max_ctx_window = evaluator.context_window_secs();
            }

            evaluators.push((now, evaluator));
        }

        let mut next_eval = now + min_evaluation_interval;

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
                                                return Err(SignalProcessError::SyncProcessTerminated(err));
                                            }
                                            SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                                // Non-recoverable error
                                                return Err(SignalProcessError::SyncProcessShutdown);
                                            }
                                        },
                                        SyncUpdate::PriceTick(_) => break,
                                        SyncUpdate::PriceHistoryState(_) => {}
                                    }
                                },
                                Err(RecvError::Lagged(skipped)) => return Err(SignalProcessError::SyncRecvLagged{skipped}),
                                Err(RecvError::Closed) => return Err(SignalProcessError::SyncRecvClosed)
                            }
                        }
                        _ = time::sleep(self.config.sync_update_timeout) => {
                            if matches!(self.sync_reader.status_snapshot(), SyncStatus::Synced) {
                                break;
                            }
                        }
                    }
                }

                now = Utc::now().ceil_sec();
            }

            let all_ctx_entries = self
                .db
                .price_ticks
                .compute_locf_entries_for_range(now, max_ctx_window)
                .await?;

            next_eval = DateTime::<Utc>::MAX_UTC;

            for (last_eval, evaluator) in evaluators.iter_mut() {
                if now < *last_eval + evaluator.evaluation_interval() {
                    continue;
                }

                *last_eval = now;

                let evaluator_next_eval = now + evaluator.evaluation_interval();
                if evaluator_next_eval < next_eval {
                    next_eval = evaluator_next_eval;
                }

                let start_idx = all_ctx_entries.len() - evaluator.context_window_secs();
                let signal_ctx_entries = &all_ctx_entries[start_idx..];

                let signal = Signal::try_evaluate(evaluator, now, signal_ctx_entries).await?;

                let _ = self.update_tx.send(signal.into());
            }
        }
    }

    pub fn spawn_recovery_loop(self) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                self.status_manager.update(LiveSignalStatusNotRunning::Starting.into());

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    run_res = self.run() => {
                        let Err(signal_error) = run_res;
                        self.status_manager.update(
                            LiveSignalStatusNotRunning::Failed(signal_error).into()
                        );
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(
                                LiveSignalStatusNotRunning::Failed(SignalProcessError::ShutdownSignalRecv(e)).into()
                            );
                        }
                        return;
                    }
                };

                self.status_manager.update(LiveSignalStatusNotRunning::Restarting.into());

                // Handle shutdown signals while waiting for `restart_interval`

                tokio::select! {
                    _ = time::sleep(self.config.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(
                                LiveSignalStatusNotRunning::Failed(SignalProcessError::ShutdownSignalRecv(e)).into()
                            );
                        }
                        return;
                    }
                }
            }
        }).into()
    }
}
