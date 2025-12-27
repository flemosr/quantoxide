use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use super::{super::core::TradingState, error::BacktestError};

/// Represents the current status of a backtest simulation process.
#[derive(Debug, Clone)]
pub enum BacktestStatus {
    /// Backtest has been created but not yet started.
    NotInitiated,
    /// Backtest is initializing and preparing to run.
    Starting,
    /// Backtest is actively running the simulation.
    Running,
    /// Backtest has completed successfully.
    Finished,
    /// Backtest encountered an error and failed.
    Failed(Arc<BacktestError>),
    /// Backtest was manually aborted by the user.
    Aborted,
}

impl BacktestStatus {
    /// Returns `true` if the backtest has not been initiated.
    pub fn is_not_initiated(&self) -> bool {
        matches!(self, Self::NotInitiated)
    }

    /// Returns `true` if the backtest is currently starting.
    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting)
    }

    /// Returns `true` if the backtest is currently running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Returns `true` if the backtest has finished successfully.
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished)
    }

    /// Returns `true` if the backtest has failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Returns `true` if the backtest was aborted.
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }

    /// Returns `true` if the backtest has stopped (finished, failed, or aborted).
    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Finished | Self::Failed(_) | Self::Aborted)
    }
}

impl fmt::Display for BacktestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::Running => write!(f, "Running"),
            Self::Finished => write!(f, "Finished"),
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Aborted => write!(f, "Aborted"),
        }
    }
}

/// Update events emitted during a backtest simulation containing status changes and trading state
/// snapshots.
#[derive(Clone)]
pub enum BacktestUpdate {
    /// Status change notification.
    Status(BacktestStatus),
    /// Trading state snapshot update.
    TradingState(TradingState),
}

impl From<BacktestStatus> for BacktestUpdate {
    fn from(value: BacktestStatus) -> Self {
        Self::Status(value)
    }
}

impl From<TradingState> for BacktestUpdate {
    fn from(value: TradingState) -> Self {
        Self::TradingState(value)
    }
}

pub(super) type BacktestTransmiter = broadcast::Sender<BacktestUpdate>;

/// Receiver for subscribing to [`BacktestUpdate`]s including status changes and trading state
/// snapshots.
pub type BacktestReceiver = broadcast::Receiver<BacktestUpdate>;

#[derive(Debug)]
pub(super) struct BacktestStatusManager {
    status: Mutex<BacktestStatus>,
    update_tx: BacktestTransmiter,
}

impl BacktestStatusManager {
    pub fn new(update_tx: BacktestTransmiter) -> Arc<Self> {
        let status = Mutex::new(BacktestStatus::NotInitiated);

        Arc::new(Self { status, update_tx })
    }

    fn lock_status(&self) -> MutexGuard<'_, BacktestStatus> {
        self.status
            .lock()
            .expect("`BacktestStatusManager` mutex can't be poisoned")
    }

    pub fn snapshot(&self) -> BacktestStatus {
        self.lock_status().clone()
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.update_tx.subscribe()
    }

    pub fn update(&self, new_status: BacktestStatus) {
        let mut status_guard = self.lock_status();
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }
}
