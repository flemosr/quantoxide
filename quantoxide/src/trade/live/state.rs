use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use lnm_sdk::api_v3::models::Trade;

use crate::{
    signal::{LiveSignalStatusNotRunning, Signal},
    sync::SyncStatusNotSynced,
};

use super::{
    super::core::TradingState,
    executor::{state::LiveTradeExecutorStatusNotReady, update::LiveTradeExecutorUpdateOrder},
    process::error::{LiveProcessFatalError, LiveProcessRecoverableError},
};

/// Represents the current status of a live trading process.
#[derive(Debug, Clone)]
pub enum LiveTradeStatus {
    /// Live trading process has been created but not yet started.
    NotInitiated,
    /// Live trading process is initializing.
    Starting,
    /// Waiting for the sync engine to reach a synced state.
    WaitingForSync(SyncStatusNotSynced),
    /// Waiting for the signal evaluator to start running.
    WaitingForSignal(LiveSignalStatusNotRunning),
    /// Waiting for the trade executor to become ready.
    WaitingTradeExecutor(LiveTradeExecutorStatusNotReady),
    /// Live trading process is actively running.
    Running,
    /// Live trading process encountered a recoverable error.
    Failed(Arc<LiveProcessRecoverableError>),
    /// Live trading process is restarting after a recoverable error.
    Restarting,
    /// Shutdown has been initiated.
    ShutdownInitiated,
    /// Live trading process has been shut down.
    Shutdown,
    /// Live trading process encountered a fatal error and terminated.
    Terminated(Arc<LiveProcessFatalError>),
}

impl fmt::Display for LiveTradeStatus {
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

impl From<LiveProcessRecoverableError> for LiveTradeStatus {
    fn from(value: LiveProcessRecoverableError) -> Self {
        Self::Failed(Arc::new(value))
    }
}

impl From<Arc<LiveProcessFatalError>> for LiveTradeStatus {
    fn from(value: Arc<LiveProcessFatalError>) -> Self {
        Self::Terminated(value)
    }
}

impl From<LiveProcessFatalError> for LiveTradeStatus {
    fn from(value: LiveProcessFatalError) -> Self {
        Arc::new(value).into()
    }
}

/// Update events emitted during live trading including status changes, signals, orders, trading
/// state, and closed trades.
#[derive(Clone)]
pub enum LiveTradeUpdate {
    /// Live trading status changed.
    Status(LiveTradeStatus),
    /// A trading signal was generated.
    Signal(Signal),
    /// A trade order operation was sent to the exchange.
    Order(LiveTradeExecutorUpdateOrder),
    /// The trading state was updated.
    TradingState(TradingState),
    /// A trade was closed.
    ClosedTrade(Trade),
}

impl From<LiveTradeStatus> for LiveTradeUpdate {
    fn from(value: LiveTradeStatus) -> Self {
        Self::Status(value)
    }
}

impl From<LiveTradeExecutorUpdateOrder> for LiveTradeUpdate {
    fn from(value: LiveTradeExecutorUpdateOrder) -> Self {
        Self::Order(value)
    }
}

impl From<Signal> for LiveTradeUpdate {
    fn from(value: Signal) -> Self {
        Self::Signal(value)
    }
}

impl From<TradingState> for LiveTradeUpdate {
    fn from(value: TradingState) -> Self {
        Self::TradingState(value)
    }
}

pub(super) type LiveTransmiter = broadcast::Sender<LiveTradeUpdate>;

/// Receiver for subscribing to [`LiveTradeUpdate`]s including status changes, signals, orders, and
/// closed trades.
pub type LiveTradeReceiver = broadcast::Receiver<LiveTradeUpdate>;

/// Trait for reading live trading status and subscribing to updates.
pub trait LiveTradeReader: Send + Sync + 'static {
    /// Creates a new [`LiveTradeReceiver`] for subscribing to live trading updates.
    fn update_receiver(&self) -> LiveTradeReceiver;

    /// Returns the current [`LiveTradeStatus`] as a snapshot.
    fn status_snapshot(&self) -> LiveTradeStatus;
}

#[derive(Debug)]
pub(super) struct LiveTradeStatusManager {
    status: Mutex<LiveTradeStatus>,
    update_tx: LiveTransmiter,
}

impl LiveTradeStatusManager {
    pub fn new(update_tx: LiveTransmiter) -> Arc<Self> {
        let status = Mutex::new(LiveTradeStatus::NotInitiated);

        Arc::new(Self { status, update_tx })
    }

    fn update_status_guard(
        &self,
        mut status_guard: MutexGuard<'_, LiveTradeStatus>,
        new_status: LiveTradeStatus,
    ) {
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    fn lock_status(&self) -> MutexGuard<'_, LiveTradeStatus> {
        self.status
            .lock()
            .expect("`LiveTradeStatusManager` mutex can't be poisoned")
    }
    pub fn update(&self, new_status: LiveTradeStatus) {
        let status_guard = self.lock_status();

        self.update_status_guard(status_guard, new_status);
    }

    pub fn update_if_not_running(&self, new_status: LiveTradeStatus) {
        let status_guard = self.lock_status();

        if matches!(*status_guard, LiveTradeStatus::Running) {
            return;
        }

        self.update_status_guard(status_guard, new_status);
    }
}

impl LiveTradeReader for LiveTradeStatusManager {
    fn update_receiver(&self) -> LiveTradeReceiver {
        self.update_tx.subscribe()
    }

    fn status_snapshot(&self) -> LiveTradeStatus {
        self.lock_status().clone()
    }
}
