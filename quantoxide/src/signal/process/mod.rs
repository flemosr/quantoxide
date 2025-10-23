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

use error::{
    ProcessResult, SignalProcessError, SignalProcessFatalError, SignalProcessRecoverableError,
};

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
                                Err(RecvError::Lagged(skipped)) => return Err(SignalProcessRecoverableError::SyncRecvLagged{skipped}.into()),
                                Err(RecvError::Closed) => return Err(SignalProcessFatalError::SyncRecvClosed.into())
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
                .await
                .map_err(SignalProcessRecoverableError::Db)?;

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
                self.status_manager
                    .update(LiveSignalStatusNotRunning::Starting.into());

                let mut shutdown_rx = self.shutdown_tx.subscribe();

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
                        self.status_manager
                            .update(LiveSignalStatusNotRunning::Failed(err).into());
                    }
                }

                self.status_manager
                    .update(LiveSignalStatusNotRunning::Restarting.into());

                // Handle shutdown signals while waiting for `restart_interval`

                tokio::select! {
                    _ = time::sleep(self.config.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(err) = shutdown_res {
                            let status = SignalProcessFatalError::ShutdownSignalRecv(err).into();
                            self.status_manager.update(status);
                        }
                        return;
                    }
                }
            }
        })
        .into()
    }
}
