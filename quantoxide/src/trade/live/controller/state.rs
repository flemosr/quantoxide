use std::{collections::HashMap, result, sync::Arc};

use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, MutexGuard, broadcast};
use uuid::Uuid;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{LnmTrade, Trade},
};

use crate::{db::DbContext, sync::SyncState};

use super::super::error::{LiveError, Result as LiveResult};

#[derive(Clone)]
pub struct LiveTradeControllerStatus {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    running: HashMap<Uuid, LnmTrade>,
    closed: Vec<LnmTrade>,
}

impl LiveTradeControllerStatus {
    pub async fn new(db: Arc<DbContext>, api: Arc<ApiContext>) -> LiveResult<Self> {
        let (trades, user) = futures::try_join!(
            api.rest()
                .futures()
                .get_trades_running(None, None, 50.into()),
            api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        let mut running = HashMap::new();
        for trade in trades.into_iter() {
            running.insert(trade.id(), trade);
        }

        Ok(Self {
            db,
            api,
            last_trade_time: None,
            balance: user.balance(),
            running,
            closed: Vec::new(),
        })
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.last_trade_time
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn running(&self) -> &HashMap<Uuid, LnmTrade> {
        &self.running
    }

    pub fn closed(&self) -> &Vec<LnmTrade> {
        &self.closed
    }

    // Returns true if the status was updated
    pub async fn reevaluate(&mut self) -> LiveResult<bool> {
        todo!()
    }

    pub fn register_trade(&mut self, new_running_trade: LnmTrade) {
        self.last_trade_time = Some(Utc::now());

        self.balance = {
            self.balance as i64
                - new_running_trade.margin().into_i64()
                - new_running_trade.maintenance_margin() as i64
        }
        .min(0) as u64;

        self.running
            .insert(new_running_trade.id(), new_running_trade);
    }

    pub fn close_trade(&mut self, closed_trade: LnmTrade) -> LiveResult<()> {
        if self.running.remove(&closed_trade.id()).is_none() {
            return Err(LiveError::Generic(
                "`closed_trade` was not running".to_string(),
            ));
        }

        self.last_trade_time = Some(Utc::now());

        self.balance = {
            self.balance as i64
                + closed_trade.margin().into_i64()
                + closed_trade.maintenance_margin() as i64
                - closed_trade.opening_fee() as i64
                - closed_trade.closing_fee() as i64
                + closed_trade.pl()
        }
        .min(0) as u64;

        self.closed.push(closed_trade);

        Ok(())
    }

    pub fn close_trades(&mut self, closed_trades: Vec<LnmTrade>) -> LiveResult<()> {
        for closed_trade in closed_trades {
            self.close_trade(closed_trade)?;
        }
        Ok(())
    }
}

pub enum LiveTradeControllerState {
    Starting,
    WaitingForSync(Arc<SyncState>),
    Ready(LiveTradeControllerStatus),
    Failed(LiveError),
    NotViable(LiveError),
}

pub struct LockedLiveTradeControllerStatus<'a> {
    state_guard: MutexGuard<'a, Arc<LiveTradeControllerState>>,
}

impl<'a> LockedLiveTradeControllerStatus<'a> {
    fn as_status(&self) -> &LiveTradeControllerStatus {
        match self.state_guard.as_ref() {
            LiveTradeControllerState::Ready(status) => status,
            _ => panic!("state must be ready"),
        }
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.as_status().last_trade_time
    }

    pub fn balance(&self) -> u64 {
        self.as_status().balance
    }

    pub fn running(&self) -> &HashMap<Uuid, LnmTrade> {
        &self.as_status().running
    }

    pub fn closed(&self) -> &Vec<LnmTrade> {
        &self.as_status().closed
    }

    pub fn to_owned(&self) -> LiveTradeControllerStatus {
        self.as_status().clone()
    }
}

impl<'a> TryFrom<MutexGuard<'a, Arc<LiveTradeControllerState>>>
    for LockedLiveTradeControllerStatus<'a>
{
    type Error = LiveError;

    fn try_from(
        value: MutexGuard<'a, Arc<LiveTradeControllerState>>,
    ) -> result::Result<Self, Self::Error> {
        match value.as_ref() {
            LiveTradeControllerState::Ready(_) => Ok(Self { state_guard: value }),
            _ => Err(LiveError::ManagerNotReady),
        }
    }
}

pub type LiveTradeControllerTransmiter = broadcast::Sender<Arc<LiveTradeControllerState>>;
pub type LiveTradeControllerReceiver = broadcast::Receiver<Arc<LiveTradeControllerState>>;

pub struct LiveTradeControllerStateManager {
    state: Mutex<Arc<LiveTradeControllerState>>,
    state_tx: LiveTradeControllerTransmiter,
}

impl LiveTradeControllerStateManager {
    pub fn new() -> Arc<Self> {
        let state = Mutex::new(Arc::new(LiveTradeControllerState::Starting));
        let (state_tx, _) = broadcast::channel::<Arc<LiveTradeControllerState>>(100);

        Arc::new(Self { state, state_tx })
    }

    pub async fn try_lock_status(&self) -> LiveResult<LockedLiveTradeControllerStatus> {
        let state_guard = self.state.lock().await;
        LockedLiveTradeControllerStatus::try_from(state_guard)
    }

    pub async fn snapshot(&self) -> Arc<LiveTradeControllerState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveTradeControllerReceiver {
        self.state_tx.subscribe()
    }

    async fn send_state_update(&self, new_state: Arc<LiveTradeControllerState>) {
        // We can safely ignore errors since they only mean that there are no
        // receivers.
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: LiveTradeControllerState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        *state_guard = new_state.clone();
        drop(state_guard);

        self.send_state_update(new_state).await
    }

    pub async fn update_status(
        &self,
        mut locked_status: LockedLiveTradeControllerStatus<'_>,
        new_status: LiveTradeControllerStatus,
    ) {
        let new_state = Arc::new(LiveTradeControllerState::Ready(new_status));

        *locked_status.state_guard = new_state.clone();
        drop(locked_status);

        self.send_state_update(new_state).await
    }
}
