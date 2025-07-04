use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use crate::sync::SyncStateNotSynced;

use super::super::{
    error::{LiveError, Result as LiveResult},
    executor::LiveTradeExecutorTransmiter,
};

mod live_trading_session;

pub use live_trading_session::LiveTradingSession;

#[derive(Debug)]
pub enum LiveTradeExecutorStatusNotReady {
    Starting,
    WaitingForSync(Arc<SyncStateNotSynced>),
    Failed(LiveError),
    NotViable(LiveError),
}

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorStatus {
    NotReady(Arc<LiveTradeExecutorStatusNotReady>),
    Ready,
}

#[derive(Debug, Clone)]
pub struct LiveTradeExecutorState {
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

    pub fn has_active_session(&self) -> bool {
        self.trading_session.is_some()
    }
}

impl From<LiveTradeExecutorStatusNotReady> for LiveTradeExecutorStatus {
    fn from(value: LiveTradeExecutorStatusNotReady) -> Self {
        Self::NotReady(Arc::new(value))
    }
}

pub struct LockedLiveTradeExecutorState<'a> {
    state_guard: MutexGuard<'a, LiveTradeExecutorState>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl<'a> LockedLiveTradeExecutorState<'a> {
    pub fn status(&self) -> &LiveTradeExecutorStatus {
        self.state_guard.status()
    }

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

pub struct LockedLiveTradeExecutorStateReady<'a>(LockedLiveTradeExecutorState<'a>);

impl<'a> TryFrom<LockedLiveTradeExecutorState<'a>> for LockedLiveTradeExecutorStateReady<'a> {
    type Error = LiveError;

    fn try_from(value: LockedLiveTradeExecutorState<'a>) -> Result<Self, Self::Error> {
        match value.state_guard.status {
            LiveTradeExecutorStatus::Ready if value.state_guard.trading_session.is_some() => {
                Ok(Self(value))
            }
            _ => Err(LiveError::ManagerNotReady),
        }
    }
}

impl<'a> LockedLiveTradeExecutorStateReady<'a> {
    pub fn trading_session(&self) -> &LiveTradingSession {
        match self.0.state_guard.status {
            LiveTradeExecutorStatus::Ready => {
                if let Some(trading_session) = self.0.state_guard.trading_session.as_ref() {
                    return trading_session;
                }
                panic!("`trading_session` must be `Some`");
            }
            _ => panic!("`LiveTradeExecutorStatus` must be ready"),
        }
    }

    // `LockeLiveTradeExecutorStateReady` is only obtained if status is
    // `LiveTradeExecutorStatus::Ready`, so the status doesn't need to be updated
    // in this case.
    pub async fn update_trading_session(self, new_trading_session: LiveTradingSession) {
        self.0.update_status_ready(new_trading_session)
    }

    pub fn update_status_not_ready(self, new_status_not_ready: LiveTradeExecutorStatusNotReady) {
        self.0.update_status_not_ready(new_status_not_ready)
    }
}

pub struct LiveTradeExecutorStateManager {
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

    pub async fn lock_state(&self) -> LockedLiveTradeExecutorState {
        let state_guard = self.state.lock().await;

        LockedLiveTradeExecutorState {
            state_guard,
            update_tx: self.update_tx.clone(),
        }
    }

    pub async fn try_lock_ready_state(&self) -> LiveResult<LockedLiveTradeExecutorStateReady> {
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

    pub async fn update_status_ready(&self, new_trading_session: LiveTradingSession) {
        self.lock_state()
            .await
            .update_status_ready(new_trading_session)
    }
}
