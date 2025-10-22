use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use crate::db::models::PriceTick;

use super::{PriceHistoryState, SyncProcessFatalError, SyncProcessRecoverableError};

#[derive(Debug)]
pub enum SyncStatusNotSynced {
    NotInitiated,
    Starting,
    InProgress,
    WaitingForResync,
    Failed(SyncProcessRecoverableError),
    Restarting,
}

impl fmt::Display for SyncStatusNotSynced {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::InProgress => write!(f, "In progress"),
            Self::WaitingForResync => write!(f, "Waiting for resync"),
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Restarting => write!(f, "Restarting"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncStatus {
    NotSynced(Arc<SyncStatusNotSynced>),
    Synced,
    ShutdownInitiated,
    Shutdown,
    Terminated(Arc<SyncProcessFatalError>),
}

impl fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSynced(status) => write!(f, "Not synced ({status})"),
            Self::Synced => write!(f, "Synced"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
            Self::Terminated(error) => write!(f, "Terminated: {error}"),
        }
    }
}

impl From<SyncStatusNotSynced> for SyncStatus {
    fn from(value: SyncStatusNotSynced) -> Self {
        Self::NotSynced(Arc::new(value))
    }
}

impl From<Arc<SyncProcessFatalError>> for SyncStatus {
    fn from(value: Arc<SyncProcessFatalError>) -> Self {
        Self::Terminated(value)
    }
}

impl From<SyncProcessFatalError> for SyncStatus {
    fn from(value: SyncProcessFatalError) -> Self {
        Arc::new(value).into()
    }
}

#[derive(Debug, Clone)]
pub enum SyncUpdate {
    Status(SyncStatus),
    PriceTick(PriceTick),
    PriceHistoryState(PriceHistoryState),
}

impl From<SyncStatus> for SyncUpdate {
    fn from(value: SyncStatus) -> Self {
        Self::Status(value)
    }
}

impl From<PriceTick> for SyncUpdate {
    fn from(value: PriceTick) -> Self {
        Self::PriceTick(value)
    }
}

impl From<PriceHistoryState> for SyncUpdate {
    fn from(value: PriceHistoryState) -> Self {
        Self::PriceHistoryState(value)
    }
}

pub type SyncTransmiter = broadcast::Sender<SyncUpdate>;
pub type SyncReceiver = broadcast::Receiver<SyncUpdate>;

pub trait SyncReader: Send + Sync + 'static {
    fn update_receiver(&self) -> SyncReceiver;
    fn status_snapshot(&self) -> SyncStatus;
}

#[derive(Debug)]
pub struct SyncStatusManager {
    status: Mutex<SyncStatus>,
    update_tx: SyncTransmiter,
}

impl SyncStatusManager {
    pub fn new(update_tx: SyncTransmiter) -> Arc<Self> {
        let status = Mutex::new(SyncStatusNotSynced::NotInitiated.into());

        Arc::new(Self { status, update_tx })
    }

    fn lock_status(&self) -> MutexGuard<'_, SyncStatus> {
        self.status
            .lock()
            .expect("`SyncStatusManager` mutex can't be poisoned")
    }

    fn update_status_guard(
        &self,
        mut status_guard: MutexGuard<'_, SyncStatus>,
        new_status: SyncStatus,
    ) {
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    pub fn update(&self, new_status: SyncStatus) {
        let status_guard = self.lock_status();

        self.update_status_guard(status_guard, new_status);
    }
}

impl SyncReader for SyncStatusManager {
    fn update_receiver(&self) -> SyncReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> SyncStatus {
        self.lock_status().clone()
    }
}
