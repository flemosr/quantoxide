use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use lnm_sdk::models::LnmTrade;

use crate::{
    signal::{LiveSignalStatusNotRunning, Signal},
    sync::SyncStatusNotSynced,
};

use super::{
    super::core::TradingState,
    executor::{state::LiveTradeExecutorStatusNotReady, update::LiveTradeExecutorUpdateOrder},
    process::error::{LiveProcessFatalError, LiveProcessRecoverableError},
};

#[derive(Debug, Clone)]
pub enum LiveStatus {
    NotInitiated,
    Starting,
    WaitingForSync(SyncStatusNotSynced),
    WaitingForSignal(LiveSignalStatusNotRunning),
    WaitingTradeExecutor(LiveTradeExecutorStatusNotReady),
    Running,
    Failed(Arc<LiveProcessRecoverableError>),
    Restarting,
    ShutdownInitiated,
    Shutdown,
    Terminated(Arc<LiveProcessFatalError>),
}

impl fmt::Display for LiveStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::WaitingForSync(status) => write!(f, "Waiting for sync ({status})"),
            Self::WaitingForSignal(status) => write!(f, "Waiting for signal ({status})"),
            Self::WaitingTradeExecutor(status) => {
                write!(f, "Waiting trade executor ({status})")
            }
            Self::Running => write!(f, "Running"),
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Restarting => write!(f, "Restarting"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
            Self::Terminated(error) => write!(f, "Terminated: {error}"),
        }
    }
}

impl From<LiveProcessRecoverableError> for LiveStatus {
    fn from(value: LiveProcessRecoverableError) -> Self {
        Self::Failed(Arc::new(value))
    }
}

impl From<Arc<LiveProcessFatalError>> for LiveStatus {
    fn from(value: Arc<LiveProcessFatalError>) -> Self {
        Self::Terminated(value)
    }
}

impl From<LiveProcessFatalError> for LiveStatus {
    fn from(value: LiveProcessFatalError) -> Self {
        Arc::new(value).into()
    }
}

#[derive(Clone)]
pub enum LiveUpdate {
    Status(LiveStatus),
    Signal(Signal),
    Order(LiveTradeExecutorUpdateOrder),
    TradingState(TradingState),
    ClosedTrade(LnmTrade),
}

impl From<LiveStatus> for LiveUpdate {
    fn from(value: LiveStatus) -> Self {
        Self::Status(value)
    }
}

impl From<LiveTradeExecutorUpdateOrder> for LiveUpdate {
    fn from(value: LiveTradeExecutorUpdateOrder) -> Self {
        Self::Order(value)
    }
}

impl From<Signal> for LiveUpdate {
    fn from(value: Signal) -> Self {
        Self::Signal(value)
    }
}

impl From<TradingState> for LiveUpdate {
    fn from(value: TradingState) -> Self {
        Self::TradingState(value)
    }
}

pub(super) type LiveTransmiter = broadcast::Sender<LiveUpdate>;
pub type LiveReceiver = broadcast::Receiver<LiveUpdate>;

pub trait LiveReader: Send + Sync + 'static {
    fn update_receiver(&self) -> LiveReceiver;
    fn status_snapshot(&self) -> LiveStatus;
}

#[derive(Debug)]
pub(super) struct LiveStatusManager {
    status: Mutex<LiveStatus>,
    update_tx: LiveTransmiter,
}

impl LiveStatusManager {
    pub fn new(update_tx: LiveTransmiter) -> Arc<Self> {
        let status = Mutex::new(LiveStatus::NotInitiated);

        Arc::new(Self { status, update_tx })
    }

    fn update_status_guard(
        &self,
        mut status_guard: MutexGuard<'_, LiveStatus>,
        new_status: LiveStatus,
    ) {
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    fn lock_status(&self) -> MutexGuard<'_, LiveStatus> {
        self.status
            .lock()
            .expect("`LiveStatusManager` mutex can't be poisoned")
    }
    pub fn update(&self, new_status: LiveStatus) {
        let status_guard = self.lock_status();

        self.update_status_guard(status_guard, new_status);
    }

    pub fn update_if_not_running(&self, new_status: LiveStatus) {
        let status_guard = self.lock_status();

        if matches!(*status_guard, LiveStatus::Running) {
            return;
        }

        self.update_status_guard(status_guard, new_status);
    }

    // pub fn update_if_running(&self, new_status: LiveStatus) {
    //     let status_guard = self.lock_status();

    //     if !matches!(*status_guard, LiveStatus::Running) {
    //         return;
    //     }

    //     self.update_status_guard(status_guard, new_status);
    // }
}

impl LiveReader for LiveStatusManager {
    fn update_receiver(&self) -> LiveReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> LiveStatus {
        self.lock_status().clone()
    }
}
