use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use crate::sync::SyncStatusNotSynced;

use super::{core::Signal, process::error::SignalProcessRecoverableError};

#[derive(Debug)]
pub enum LiveSignalStatusNotRunning {
    NotInitiated,
    Starting,
    WaitingForSync(Arc<SyncStatusNotSynced>),
    Failed(SignalProcessRecoverableError),
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

#[derive(Debug, Clone)]
pub enum LiveSignalStatus {
    NotRunning(Arc<LiveSignalStatusNotRunning>),
    Running,
    ShutdownInitiated,
    Shutdown,
}

impl fmt::Display for LiveSignalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning(status) => write!(f, "Not running ({status})"),
            Self::Running => write!(f, "Running"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
        }
    }
}

impl From<LiveSignalStatusNotRunning> for LiveSignalStatus {
    fn from(value: LiveSignalStatusNotRunning) -> Self {
        Self::NotRunning(Arc::new(value))
    }
}

#[derive(Debug, Clone)]
pub enum LiveSignalUpdate {
    Status(LiveSignalStatus),
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

pub type LiveSignalTransmiter = broadcast::Sender<LiveSignalUpdate>;
pub type LiveSignalReceiver = broadcast::Receiver<LiveSignalUpdate>;

pub trait LiveSignalReader: Send + Sync + 'static {
    fn update_receiver(&self) -> LiveSignalReceiver;
    fn status_snapshot(&self) -> LiveSignalStatus;
}

#[derive(Debug)]
pub struct LiveSignalStatusManager {
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
