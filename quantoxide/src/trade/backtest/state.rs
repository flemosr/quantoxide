use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use super::{super::core::TradingState, error::BacktestError};

#[derive(Debug, Clone)]
pub enum BacktestStatus {
    NotInitiated,
    Starting,
    Running,
    Finished,
    Failed(Arc<BacktestError>),
    Aborted,
}

impl BacktestStatus {
    pub fn is_not_initiated(&self) -> bool {
        matches!(self, Self::NotInitiated)
    }

    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting)
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
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

#[derive(Clone)]
pub enum BacktestUpdate {
    Status(BacktestStatus),
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

pub type BacktestTransmiter = broadcast::Sender<BacktestUpdate>;
pub type BacktestReceiver = broadcast::Receiver<BacktestUpdate>;

#[derive(Debug)]
pub struct BacktestStatusManager {
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
