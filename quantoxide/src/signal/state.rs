use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use crate::sync::SyncStatusNotSynced;

use super::{
    core::Signal,
    process::error::{SignalProcessFatalError, SignalProcessRecoverableError},
};

/// Detailed status when signal evaluation is not actively running.
///
/// Represents various states during the signal process lifecycle before achieving active signal
/// evaluation.
#[derive(Debug, Clone)]
pub enum LiveSignalStatusNotRunning {
    /// Signal process has not been started yet.
    NotInitiated,
    /// Signal process is initializing.
    Starting,
    /// Signal process is waiting for sync to complete before evaluating signals.
    WaitingForSync(SyncStatusNotSynced),
    /// Signal process encountered a recoverable error.
    Failed(Arc<SignalProcessRecoverableError>),
    /// Signal process is restarting after an error.
    Restarting,
}

impl fmt::Display for LiveSignalStatusNotRunning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::WaitingForSync(status) => {
                write!(f, "Waiting for sync ({status})")
            }
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Restarting => write!(f, "Restarting"),
        }
    }
}

/// Overall status of the live signal evaluation process.
///
/// Represents the high-level state of the signal process, including active evaluation, completion,
/// and shutdown states.
#[derive(Debug, Clone)]
pub enum LiveSignalStatus {
    /// Signal evaluation is not actively running.
    NotRunning(LiveSignalStatusNotRunning),
    /// Signal evaluation is actively running and generating signals.
    Running,
    /// Shutdown has been requested and is in progress.
    ShutdownInitiated,
    /// Signal process has been gracefully shut down.
    Shutdown,
    /// Signal process terminated due to a fatal error.
    Terminated(Arc<SignalProcessFatalError>),
}

impl fmt::Display for LiveSignalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning(status) => write!(f, "Not running ({status})"),
            Self::Running => write!(f, "Running"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
            Self::Terminated(error) => write!(f, "Terminated: {error}"),
        }
    }
}

impl From<LiveSignalStatusNotRunning> for LiveSignalStatus {
    fn from(value: LiveSignalStatusNotRunning) -> Self {
        Self::NotRunning(value)
    }
}

impl From<SignalProcessRecoverableError> for LiveSignalStatus {
    fn from(value: SignalProcessRecoverableError) -> Self {
        LiveSignalStatusNotRunning::Failed(Arc::new(value)).into()
    }
}

impl From<Arc<SignalProcessFatalError>> for LiveSignalStatus {
    fn from(value: Arc<SignalProcessFatalError>) -> Self {
        Self::Terminated(value)
    }
}

impl From<SignalProcessFatalError> for LiveSignalStatus {
    fn from(value: SignalProcessFatalError) -> Self {
        Arc::new(value).into()
    }
}

/// Update events emitted by the live signal evaluation process.
///
/// These updates are broadcast to subscribers and include status changes and newly generated
/// trading signals.
#[derive(Debug, Clone)]
pub enum LiveSignalUpdate {
    /// Signal process status has changed.
    Status(LiveSignalStatus),
    /// A new trading signal has been generated.
    Signal(Signal),
}

impl From<LiveSignalStatus> for LiveSignalUpdate {
    fn from(value: LiveSignalStatus) -> Self {
        Self::Status(value)
    }
}

impl From<Signal> for LiveSignalUpdate {
    fn from(value: Signal) -> Self {
        Self::Signal(value)
    }
}

pub(super) type LiveSignalTransmiter = broadcast::Sender<LiveSignalUpdate>;

/// Receiver for subscribing to [`LiveSignalUpdate`]s.
pub type LiveSignalReceiver = broadcast::Receiver<LiveSignalUpdate>;

/// Trait for reading signal evaluation status and subscribing to updates.
///
/// Provides a read-only interface to the signal process state without the ability to control or
/// modify it.
pub trait LiveSignalReader: Send + Sync + 'static {
    /// Creates a new [`LiveSignalReceiver`] for subscribing to signal updates.
    fn update_receiver(&self) -> LiveSignalReceiver;

    /// Returns the current [`LiveSignalStatus`] as a snapshot.
    fn status_snapshot(&self) -> LiveSignalStatus;
}

#[derive(Debug)]
pub(super) struct LiveSignalStatusManager {
    status: Mutex<LiveSignalStatus>,
    update_tx: LiveSignalTransmiter,
}

impl LiveSignalStatusManager {
    pub fn new(update_tx: LiveSignalTransmiter) -> Arc<Self> {
        let status = Mutex::new(LiveSignalStatusNotRunning::NotInitiated.into());

        Arc::new(Self { status, update_tx })
    }

    fn lock_status(&self) -> MutexGuard<'_, LiveSignalStatus> {
        self.status
            .lock()
            .expect("`LiveSignalStatusManager` mutex can't be poisoned")
    }
    pub fn update(&self, new_status: LiveSignalStatus) {
        let mut status_guard = self.lock_status();
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }
}

impl LiveSignalReader for LiveSignalStatusManager {
    fn update_receiver(&self) -> LiveSignalReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> LiveSignalStatus {
        self.lock_status().clone()
    }
}
