use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use crate::{db::models::PriceTickRow, sync::SyncMode};

use super::process::{
    error::{SyncProcessFatalError, SyncProcessRecoverableError},
    sync_price_history_task::price_history_state::PriceHistoryState,
};

/// Detailed status when synchronization is not yet complete.
///
/// Represents various states during the synchronization process before achieving full sync.
#[derive(Debug, Clone)]
pub enum SyncStatusNotSynced {
    /// Sync process has not been started yet.
    NotInitiated,
    /// Sync process is initializing.
    Starting,
    /// Sync process is actively fetching and processing data.
    InProgress,
    /// Sync process is waiting for the next resync interval.
    WaitingForResync,
    /// Sync process encountered a recoverable error.
    Failed(Arc<SyncProcessRecoverableError>),
    /// Sync process is restarting after an error.
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

/// Overall synchronization status.
///
/// Represents the high-level state of the sync process, including active synchronization,
/// completion, and shutdown states.
#[derive(Debug, Clone)]
pub enum SyncStatus {
    /// Synchronization is in progress but not yet complete.
    NotSynced(SyncStatusNotSynced),
    /// Synchronization has been successfully completed.
    Synced,
    /// Shutdown has been requested and is in progress.
    ShutdownInitiated,
    /// Sync process has been gracefully shut down.
    Shutdown,
    /// Sync process terminated due to a fatal error.
    Terminated(Arc<SyncProcessFatalError>),
}

impl SyncStatus {
    /// Returns `true` if the sync process has stopped (either shut down or terminated).
    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Shutdown | Self::Terminated(_))
    }
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
        Self::NotSynced(value)
    }
}

impl From<SyncProcessRecoverableError> for SyncStatus {
    fn from(value: SyncProcessRecoverableError) -> Self {
        SyncStatusNotSynced::Failed(Arc::new(value)).into()
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

/// Update events emitted by the synchronization process.
///
/// These updates are broadcast to subscribers and include status changes, new price ticks, and
/// price history state evaluations.
#[derive(Debug, Clone)]
pub enum SyncUpdate {
    /// Sync status has changed.
    Status(SyncStatus),
    /// A new price tick has been received.
    PriceTick(PriceTickRow),
    /// Price history state has been evaluated or updated.
    PriceHistoryState(PriceHistoryState),
}

impl From<SyncStatus> for SyncUpdate {
    fn from(value: SyncStatus) -> Self {
        Self::Status(value)
    }
}

impl From<PriceTickRow> for SyncUpdate {
    fn from(value: PriceTickRow) -> Self {
        Self::PriceTick(value)
    }
}

impl From<PriceHistoryState> for SyncUpdate {
    fn from(value: PriceHistoryState) -> Self {
        Self::PriceHistoryState(value)
    }
}

pub(super) type SyncTransmiter = broadcast::Sender<SyncUpdate>;

/// Receiver for subscribing to [`SyncUpdate`]s.
pub type SyncReceiver = broadcast::Receiver<SyncUpdate>;

/// Trait for reading synchronization status and subscribing to updates.
///
/// Provides a read-only interface to the sync process state without the ability to control or
/// modify it.
pub trait SyncReader: Send + Sync + 'static {
    /// Returns the [`SyncMode`] of the sync process.
    fn mode(&self) -> SyncMode;

    /// Creates a new [`SyncReceiver`] for subscribing to sync updates.
    fn update_receiver(&self) -> SyncReceiver;

    /// Returns the current [`SyncStatus`] a snapshot.
    fn status_snapshot(&self) -> SyncStatus;
}

#[derive(Debug)]
pub(super) struct SyncStatusManager {
    mode: SyncMode,
    status: Mutex<SyncStatus>,
    update_tx: SyncTransmiter,
}

impl SyncStatusManager {
    pub fn new(mode: SyncMode, update_tx: SyncTransmiter) -> Arc<Self> {
        let status = Mutex::new(SyncStatusNotSynced::NotInitiated.into());

        Arc::new(Self {
            mode,
            status,
            update_tx,
        })
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
    fn mode(&self) -> SyncMode {
        self.mode
    }

    fn update_receiver(&self) -> SyncReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> SyncStatus {
        self.lock_status().clone()
    }
}
