use std::{collections::HashMap, result, sync::Arc};

use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use lnm_sdk::api::rest::models::LnmTrade;

use crate::{sync::SyncStateNotSynced, trade::core::TradeTrailingStoploss};

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

pub struct LockedLiveTradingSession<'a> {
    state_guard: MutexGuard<'a, LiveTradeExecutorState>,
}

impl<'a> TryFrom<MutexGuard<'a, LiveTradeExecutorState>> for LockedLiveTradingSession<'a> {
    type Error = LiveError;

    fn try_from(
        value: MutexGuard<'a, LiveTradeExecutorState>,
    ) -> result::Result<Self, Self::Error> {
        match value.status {
            LiveTradeExecutorStatus::Ready if value.trading_session.is_some() => {
                Ok(Self { state_guard: value })
            }
            _ => Err(LiveError::ManagerNotReady),
        }
    }
}

impl<'a> LockedLiveTradingSession<'a> {
    fn as_session(&self) -> &LiveTradingSession {
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

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.as_session().last_trade_time()
    }

    pub fn balance(&self) -> u64 {
        self.as_session().balance()
    }

    pub fn running(&self) -> &HashMap<Uuid, (Arc<LnmTrade>, Option<TradeTrailingStoploss>)> {
        self.as_session().running()
    }

    pub fn closed(&self) -> &Vec<Arc<LnmTrade>> {
        self.as_session().closed()
    }

    pub fn to_owned(&self) -> LiveTradingSession {
        self.as_session().clone()
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

    pub async fn try_lock_trading_session(&self) -> LiveResult<LockedLiveTradingSession> {
        let state_guard = self.state.lock().await;
        LockedLiveTradingSession::try_from(state_guard)
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

    // `LockedLiveTradingSession` is only obtained if status is `LiveTradeExecutorStatus::Ready`,
    // so the status doesn't need to be updated in this case.
    pub async fn update_locked_trading_session(
        &self,
        mut locked_trading_session: LockedLiveTradingSession<'_>,
        new_trading_session: LiveTradingSession,
    ) {
        locked_trading_session.state_guard.trading_session = Some(new_trading_session.clone());
        drop(locked_trading_session);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_trading_session.into());
    }

    pub fn update_locked_status_not_ready(
        &self,
        mut locked_trading_session: LockedLiveTradingSession<'_>,
        new_status_not_ready: LiveTradeExecutorStatusNotReady,
    ) {
        let new_status: LiveTradeExecutorStatus = new_status_not_ready.into();

        locked_trading_session.state_guard.status = new_status.clone();
        drop(locked_trading_session);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }
}
