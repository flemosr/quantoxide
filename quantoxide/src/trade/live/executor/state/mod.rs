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

pub struct LockedLiveTradeExecutorStateReady<'a> {
    state_guard: MutexGuard<'a, LiveTradeExecutorState>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl<'a> LockedLiveTradeExecutorStateReady<'a> {
    pub fn trading_session(&self) -> &LiveTradingSession {
        match self.state_guard.status {
            LiveTradeExecutorStatus::Ready => {
                if let Some(trading_session) = self.state_guard.trading_session.as_ref() {
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
    pub async fn update_trading_session(mut self, new_trading_session: LiveTradingSession) {
        self.state_guard.trading_session = Some(new_trading_session.clone());

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_trading_session.into());
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

    pub async fn try_lock_ready_state(&self) -> LiveResult<LockedLiveTradeExecutorStateReady> {
        let state_guard = self.state.lock().await;

        match state_guard.status {
            LiveTradeExecutorStatus::Ready if state_guard.trading_session.is_some() => {
                Ok(LockedLiveTradeExecutorStateReady {
                    state_guard,
                    update_tx: self.update_tx.clone(),
                })
            }
            _ => Err(LiveError::ManagerNotReady),
        }
    }

    pub async fn snapshot(&self) -> LiveTradeExecutorState {
        self.state.lock().await.clone()
    }

    pub async fn update_status_not_ready(
        &self,
        new_status_not_ready: LiveTradeExecutorStatusNotReady,
    ) {
        let new_status: LiveTradeExecutorStatus = new_status_not_ready.into();

        let mut state_guard = self.state.lock().await;
        state_guard.status = new_status.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }

    pub async fn update_status_ready(&self, new_trading_session: LiveTradingSession) {
        let mut state_guard = self.state.lock().await;

        let new_status = LiveTradeExecutorStatus::Ready;

        state_guard.status = new_status.clone();
        state_guard.trading_session = Some(new_trading_session.clone());
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
        let _ = self.update_tx.send(new_trading_session.into());
    }
}
