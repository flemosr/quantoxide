use std::{collections::HashMap, result, sync::Arc};

use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, MutexGuard, broadcast};
use uuid::Uuid;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{LnmTrade, Trade},
};

use crate::{db::DbContext, sync::SyncState, trade::core::PriceTrigger};

use super::super::error::{LiveError, Result as LiveResult};

#[derive(Clone)]
pub struct LiveTradeControllerStatus {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    trigger: PriceTrigger,
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

        let mut last_trade_time: Option<DateTime<Utc>> = None;
        let mut trigger = PriceTrigger::NotSet;
        let mut running = HashMap::new();

        for trade in trades.into_iter() {
            let new_last_trade_time = if let Some(last_trade_time) = last_trade_time {
                last_trade_time.max(trade.creation_ts())
            } else {
                trade.creation_ts()
            };
            last_trade_time = Some(new_last_trade_time);
            trigger.update(&trade);
            running.insert(trade.id(), trade);
        }

        Ok(Self {
            db,
            api,
            last_trade_time,
            balance: user.balance(),
            trigger,
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

    pub fn register_running_trade(&mut self, new_trade: LnmTrade) -> LiveResult<()> {
        if !new_trade.running() {
            return Err(LiveError::Generic(format!(
                "`new_trade` {} is not running",
                new_trade.id(),
            )));
        }

        if !self.running.contains_key(&new_trade.id()) {
            return Err(LiveError::Generic(format!(
                "`new_trade` {} already registered",
                new_trade.id(),
            )));
        }

        if self
            .last_trade_time
            .map_or(false, |last| last >= new_trade.creation_ts())
        {
            return Err(LiveError::Generic(format!(
                "`new_trade` {} `creation_ts` is not gt than `last_trade_time`",
                new_trade.id(),
            )));
        }

        self.last_trade_time = Some(new_trade.creation_ts());

        self.balance = {
            self.balance as i64
                - new_trade.margin().into_i64()
                - new_trade.maintenance_margin() as i64
        }
        .min(0) as u64;

        self.trigger.update(&new_trade);
        self.running.insert(new_trade.id(), new_trade);

        Ok(())
    }

    pub fn close_trades(&mut self, closed_trades: Vec<LnmTrade>) -> LiveResult<()> {
        let mut closed_map = HashMap::new();
        let mut new_last_trade_time: Option<DateTime<Utc>> = None;

        for closed_trade in closed_trades {
            let closed_ts = if let Some(closed_ts) = closed_trade.closed_ts() {
                closed_ts
            } else {
                return Err(LiveError::Generic(format!(
                    "`closed_trade` {} is not closed",
                    closed_trade.id(),
                )));
            };

            if !self.running.contains_key(&closed_trade.id()) {
                return Err(LiveError::Generic(format!(
                    "`closed_trade` {} was not running",
                    closed_trade.id(),
                )));
            }

            if self.last_trade_time.map_or(false, |last| last >= closed_ts) {
                return Err(LiveError::Generic(format!(
                    "`closed_trade` {} `closed_ts` is not gt than `last_trade_time`",
                    closed_trade.id(),
                )));
            }

            closed_map.insert(closed_trade.id(), closed_trade);

            if new_last_trade_time.map_or(true, |last_closed| closed_ts > last_closed) {
                new_last_trade_time = Some(closed_ts);
            }
        }

        let mut new_running = HashMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;

        for (id, trade) in &self.running {
            if let Some(closed_trade) = closed_map.remove(id) {
                new_balance += closed_trade.margin().into_i64()
                    + closed_trade.maintenance_margin() as i64
                    - closed_trade.opening_fee() as i64
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                self.closed.push(closed_trade);
            } else {
                new_trigger.update(trade);
                new_running.insert(*id, trade.clone());
            }
        }

        self.last_trade_time = new_last_trade_time;
        self.trigger = new_trigger;
        self.running = new_running;
        self.balance = new_balance.min(0) as u64;

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
