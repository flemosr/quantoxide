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

/// Update events emitted during a single-operator backtest simulation containing status changes
/// and trading state snapshots.
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

pub(super) type BacktestTransmitter = broadcast::Sender<BacktestUpdate>;

/// Receiver for subscribing to [`BacktestUpdate`]s including status changes and trading
/// state snapshots.
pub type BacktestReceiver = broadcast::Receiver<BacktestUpdate>;

/// Update events for parallel backtest containing status changes and per-operator trading
/// state snapshots.
#[derive(Clone)]
pub enum BacktestParallelUpdate {
    /// Status change notification.
    Status(BacktestStatus),
    /// Trading state snapshot for a specific operator.
    TradingState {
        /// The name of the operator this state belongs to.
        operator_name: String,
        /// The trading state snapshot.
        state: Box<TradingState>,
    },
}

impl From<BacktestStatus> for BacktestParallelUpdate {
    fn from(value: BacktestStatus) -> Self {
        Self::Status(value)
    }
}

pub(super) type BacktestParallelTransmitter = broadcast::Sender<BacktestParallelUpdate>;

/// Receiver for subscribing to [`BacktestParallelUpdate`]s including status changes and per-operator
/// trading state snapshots.
pub type BacktestParallelReceiver = broadcast::Receiver<BacktestParallelUpdate>;

pub(super) struct BacktestStatusManager<T: Clone> {
    status: Mutex<BacktestStatus>,
    update_tx: broadcast::Sender<T>,
}

impl<T: Clone> fmt::Debug for BacktestStatusManager<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BacktestStatusManager")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl<T: Clone + From<BacktestStatus>> BacktestStatusManager<T> {
    pub fn new(update_tx: broadcast::Sender<T>) -> Arc<Self> {
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

    pub fn receiver(&self) -> broadcast::Receiver<T> {
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
