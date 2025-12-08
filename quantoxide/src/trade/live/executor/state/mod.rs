use std::{fmt, sync::Arc};

use tokio::sync::{Mutex, MutexGuard};

use crate::sync::SyncStatusNotSynced;

use super::{
    error::{
        ExecutorActionError, ExecutorActionResult, ExecutorProcessFatalError,
        ExecutorProcessRecoverableError,
    },
    update::LiveTradeExecutorTransmiter,
};

pub(in crate::trade) mod live_trading_session;

use live_trading_session::LiveTradingSession;

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorStatusNotReady {
    Starting,
    WaitingForSync(SyncStatusNotSynced),
    Failed(Arc<ExecutorProcessRecoverableError>),
    Terminated(Arc<ExecutorProcessFatalError>),
    ShutdownInitiated,
    Shutdown,
}

impl fmt::Display for LiveTradeExecutorStatusNotReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Starting => write!(f, "Starting"),
            Self::WaitingForSync(status) => {
                write!(f, "Waiting for sync ({status})")
            }
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Terminated(error) => write!(f, "Terminated: {error}"),
            Self::ShutdownInitiated => write!(f, "Shutdown initiated"),
            Self::Shutdown => write!(f, "Shutdown"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorStatus {
    NotReady(LiveTradeExecutorStatusNotReady),
    Ready,
}

impl fmt::Display for LiveTradeExecutorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady(status) => write!(f, "Not ready ({})", status),
            Self::Ready => write!(f, "Ready"),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::trade) struct LiveTradeExecutorState {
    status: LiveTradeExecutorStatus,
    trading_session: Option<LiveTradingSession>,
}

impl LiveTradeExecutorState {
    pub fn status(&self) -> &LiveTradeExecutorStatus {
        &self.status
    }

    pub fn trading_session(&self) -> Option<&LiveTradingSession> {
        self.trading_session.as_ref()
    }
}

impl From<LiveTradeExecutorStatusNotReady> for LiveTradeExecutorStatus {
    fn from(value: LiveTradeExecutorStatusNotReady) -> Self {
        Self::NotReady(value)
    }
}

pub(super) struct LockedLiveTradeExecutorState<'a> {
    state_guard: MutexGuard<'a, LiveTradeExecutorState>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl<'a> LockedLiveTradeExecutorState<'a> {
    pub fn trading_session(&self) -> Option<&LiveTradingSession> {
        self.state_guard.trading_session()
    }

    pub fn update_status_not_ready(
        mut self,
        new_status_not_ready: LiveTradeExecutorStatusNotReady,
    ) {
        let new_status: LiveTradeExecutorStatus = new_status_not_ready.into();

        self.state_guard.status = new_status.clone();

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    pub fn update_status_ready(mut self, new_trading_session: LiveTradingSession) {
        if !matches!(self.state_guard.status, LiveTradeExecutorStatus::Ready) {
            self.state_guard.status = LiveTradeExecutorStatus::Ready;
            let _ = self.update_tx.send(LiveTradeExecutorStatus::Ready.into());
        }

        self.state_guard.trading_session = Some(new_trading_session.clone());
        let _ = self.update_tx.send(new_trading_session.into());
    }
}

/// Represents a locked live trade executor in the Ready state with an active
/// trading session.
pub(super) struct LockedLiveTradeExecutorStateReady<'a>(LockedLiveTradeExecutorState<'a>);

impl<'a> TryFrom<LockedLiveTradeExecutorState<'a>> for LockedLiveTradeExecutorStateReady<'a> {
    type Error = ExecutorActionError;

    /// Attempts to convert a generic locked state into a ready state.
    ///
    /// # Errors
    ///
    /// Returns `LiveTradeExecutorError` if:
    /// - The executor status is not `Ready`
    /// - The trading session is `None`
    fn try_from(value: LockedLiveTradeExecutorState<'a>) -> Result<Self, Self::Error> {
        match value.state_guard.status {
            LiveTradeExecutorStatus::Ready if value.state_guard.trading_session.is_some() => {
                Ok(Self(value))
            }
            _ => Err(ExecutorActionError::ExecutorNotReady),
        }
    }
}

impl<'a> LockedLiveTradeExecutorStateReady<'a> {
    /// Returns a reference to the active trading session.
    ///
    /// # Panics
    ///
    /// This should never panic due to the guarantees provided by the `TryFrom`
    /// implementation, but includes an assertion for defensive programming.
    pub fn trading_session(&self) -> &LiveTradingSession {
        if !matches!(self.0.state_guard.status, LiveTradeExecutorStatus::Ready) {
            panic!("Must be `LiveTradeExecutorStatus::Ready` from `TryFrom`");
        }
        self.0.state_guard.trading_session.as_ref().unwrap()
    }

    pub async fn update_trading_session(self, new_trading_session: LiveTradingSession) {
        self.0.update_status_ready(new_trading_session)
    }
}

pub(super) struct LiveTradeExecutorStateManager {
    state: Mutex<LiveTradeExecutorState>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl LiveTradeExecutorStateManager {
    pub fn new(update_tx: LiveTradeExecutorTransmiter) -> Arc<Self> {
        let initial_state = LiveTradeExecutorState {
            status: LiveTradeExecutorStatusNotReady::Starting.into(),
            trading_session: None,
        };
        let state = Mutex::new(initial_state);

        Arc::new(Self { state, update_tx })
    }

    pub async fn lock_state(&self) -> LockedLiveTradeExecutorState<'_> {
        let state_guard = self.state.lock().await;

        LockedLiveTradeExecutorState {
            state_guard,
            update_tx: self.update_tx.clone(),
        }
    }

    pub async fn try_lock_ready_state(
        &self,
    ) -> ExecutorActionResult<LockedLiveTradeExecutorStateReady<'_>> {
        let locked_state = self.lock_state().await;
        LockedLiveTradeExecutorStateReady::try_from(locked_state)
    }

    pub async fn snapshot(&self) -> LiveTradeExecutorState {
        self.state.lock().await.clone()
    }

    pub async fn update_status_not_ready(
        &self,
        new_status_not_ready: LiveTradeExecutorStatusNotReady,
    ) {
        self.lock_state()
            .await
            .update_status_not_ready(new_status_not_ready)
    }

    pub async fn has_registered_running_trades(&self) -> bool {
        self.lock_state()
            .await
            .trading_session()
            .is_some_and(|session| !session.running_map().is_empty())
    }
}
