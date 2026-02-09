use std::sync::{Arc, Mutex};

use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};

use crate::{
    db::Database,
    signal::{
        SignalEvaluator,
        config::{LiveSignalConfig, LiveSignalControllerConfig},
        error::SignalOperatorError,
        process::{LiveSignalProcess, error::SignalProcessFatalError},
    },
    sync::SyncReader,
    util::AbortOnDropHandle,
};

use super::{
    core::{Signal, WrappedSignalEvaluator},
    error::{Result, SignalError},
    state::{
        LiveSignalReader, LiveSignalReceiver, LiveSignalStatus, LiveSignalStatusManager,
        LiveSignalTransmitter, LiveSignalUpdate,
    },
};

/// Controller for managing and monitoring a running signal evaluation process.
///
/// `LiveSignalController` provides an interface to monitor the status of signal evaluation and
/// perform graceful shutdown operations. It holds a handle to the running signal task and
/// coordinates shutdown signals.
#[derive(Debug)]
pub struct LiveSignalController<S: Signal> {
    config: LiveSignalControllerConfig,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<LiveSignalStatusManager<S>>,
}

impl<S: Signal> LiveSignalController<S> {
    fn new(
        config: &LiveSignalConfig,
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<LiveSignalStatusManager<S>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            status_manager,
        })
    }

    /// Returns a [`LiveSignalReader`](crate::signal::LiveSignalReader) interface for accessing
    /// signal status and updates.
    pub fn reader(&self) -> Arc<dyn LiveSignalReader<S>> {
        self.status_manager.clone()
    }

    /// Creates a new [`LiveSignalReceiver`] for subscribing to signal status updates and new
    /// signals.
    pub fn update_receiver(&self) -> LiveSignalReceiver<S> {
        self.status_manager.update_receiver()
    }

    /// Returns the current [`LiveSignalStatus`] as a snapshot.
    pub fn status_snapshot(&self) -> LiveSignalStatus {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`LiveSignalController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the live signal process and consumes the task handle.
    ///
    /// If a clean shutdown fails, the process is aborted. This method can only be called once per
    /// controller instance.
    ///
    /// Returns an error if the process had to be aborted, or if the handle was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(SignalError::LiveSignalAlreadyShutdown);
        };

        if handle.is_finished() {
            let status = self.status_manager.status_snapshot();
            return Err(SignalError::LiveSignalAlreadyTerminated(status));
        }

        self.status_manager
            .update(LiveSignalStatus::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SignalProcessFatalError::SendShutdownSignalFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SignalProcessFatalError::LiveSignalProcessTaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout()) => {
                        handle.abort();
                        Err(SignalProcessFatalError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        if let Err(e) = shutdown_res {
            let e_ref = Arc::new(e);
            self.status_manager.update(e_ref.clone().into());

            return Err(SignalError::SignalShutdownFailed(e_ref));
        }

        self.status_manager.update(LiveSignalStatus::Shutdown);
        Ok(())
    }

    /// Waits until the signal process has stopped and returns the final status.
    ///
    /// This method blocks until the signal process reaches a stopped state, either through graceful
    /// shutdown or termination.
    pub async fn until_stopped(&self) -> LiveSignalStatus {
        let mut signal_rx = self.update_receiver();

        let status = self.status_snapshot();
        if status.is_stopped() {
            return status;
        }

        loop {
            match signal_rx.recv().await {
                Ok(signal_update) => {
                    if let LiveSignalUpdate::Status(status) = signal_update
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

/// Builder for configuring and starting a live signal evaluation engine.
///
/// `LiveSignalEngine` encapsulates the configuration, database connection, sync reader, and signal
/// evaluators. The signal process is spawned when [`start`](Self::start) is called, and a
/// [`LiveSignalController`] is returned for monitoring and management.
pub struct LiveSignalEngine<S: Signal> {
    config: LiveSignalConfig,
    db: Arc<Database>,
    sync_reader: Arc<dyn SyncReader>,
    evaluators: Vec<WrappedSignalEvaluator<S>>,
    status_manager: Arc<LiveSignalStatusManager<S>>,
    update_tx: LiveSignalTransmitter<S>,
}

impl<S: Signal> LiveSignalEngine<S> {
    /// Creates a new live signal engine with the specified configuration and signal evaluators.
    pub fn new(
        config: impl Into<LiveSignalConfig>,
        db: Arc<Database>,
        sync_reader: Arc<dyn SyncReader>,
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(SignalError::Operator(
                SignalOperatorError::EmptyEvaluatorsVec,
            ));
        }

        let evaluators: Vec<_> = evaluators
            .into_iter()
            .map(WrappedSignalEvaluator::new)
            .collect();

        let (update_tx, _) = broadcast::channel::<LiveSignalUpdate<S>>(1_000);

        let status_manager = LiveSignalStatusManager::new(update_tx.clone());

        Ok(Self {
            config: config.into(),
            db,
            sync_reader,
            evaluators,
            status_manager,
            update_tx,
        })
    }

    /// Returns a reader interface for accessing signal status and updates.
    pub fn reader(&self) -> Arc<dyn LiveSignalReader<S>> {
        self.status_manager.clone()
    }

    /// Creates a new receiver for subscribing to signal status updates and new signals.
    pub fn update_receiver(&self) -> LiveSignalReceiver<S> {
        self.status_manager.update_receiver()
    }

    /// Returns the current signal evaluation status as a snapshot.
    pub fn status_snapshot(&self) -> LiveSignalStatus {
        self.status_manager.status_snapshot()
    }

    /// Starts the signal evaluation process and returns a [`LiveSignalController`] for managing it.
    ///
    /// This consumes the engine and spawns the signal task in the background.
    pub fn start(self) -> Arc<LiveSignalController<S>> {
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let handle = LiveSignalProcess::spawn(
            &self.config,
            self.db,
            self.evaluators,
            shutdown_tx.clone(),
            self.sync_reader,
            self.status_manager.clone(),
            self.update_tx,
        );

        LiveSignalController::new(&self.config, handle, shutdown_tx, self.status_manager)
    }
}
