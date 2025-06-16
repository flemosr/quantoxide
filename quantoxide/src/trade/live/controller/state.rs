use std::{collections::HashMap, result, sync::Arc};

use chrono::{DateTime, Utc};
use futures::future;
use tokio::sync::{Mutex, MutexGuard, broadcast};
use uuid::Uuid;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{BoundedPercentage, LnmTrade, Trade},
};

use crate::{
    db::DbContext,
    sync::SyncState,
    trade::core::{PriceTrigger, TradeExt},
};

use super::super::error::{LiveError, Result as LiveResult};

#[derive(Debug, Clone)]
pub struct LiveTradeControllerReadyStatus {
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    last_evaluation_time: DateTime<Utc>,
    trigger: PriceTrigger,
    running: HashMap<Uuid, (LnmTrade, Option<BoundedPercentage>)>,
    closed: Vec<LnmTrade>,
}

impl LiveTradeControllerReadyStatus {
    pub async fn new(db: &DbContext, api: &ApiContext) -> LiveResult<Self> {
        let (lastest_entry_time, _) = db
            .price_ticks
            .get_latest_entry()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        let (running_trades, user) = futures::try_join!(
            api.rest.futures.get_trades_running(None, None, None),
            api.rest.user.get_user()
        )
        .map_err(LiveError::RestApi)?;

        let mut last_trade_time: Option<DateTime<Utc>> = None;
        let mut trigger = PriceTrigger::NotSet;
        let mut running = HashMap::new();

        for trade in running_trades.into_iter() {
            let new_last_trade_time = if let Some(last_trade_time) = last_trade_time {
                last_trade_time.max(trade.creation_ts())
            } else {
                trade.creation_ts()
            };
            last_trade_time = Some(new_last_trade_time);
            trigger
                .update(&trade, None)
                .map_err(|e| LiveError::Generic(e.to_string()))?;
            running.insert(trade.id(), (trade, None));
        }

        Ok(Self {
            last_trade_time,
            balance: user.balance(),
            last_evaluation_time: lastest_entry_time,
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

    pub fn running(&self) -> &HashMap<Uuid, (LnmTrade, Option<BoundedPercentage>)> {
        &self.running
    }

    pub fn closed(&self) -> &Vec<LnmTrade> {
        &self.closed
    }

    // Returns true if the status was updated
    pub async fn reevaluate(&mut self, db: &DbContext, api: &ApiContext) -> LiveResult<bool> {
        let (new_evaluation_time, range_min, range_max) = db
            .price_ticks
            .get_price_range_from(self.last_evaluation_time)
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        if !self.trigger.was_reached(range_min) && !self.trigger.was_reached(range_max) {
            // General trigger was not reached. No trades need to be checked
            self.last_evaluation_time = new_evaluation_time;
            return Ok(false);
        }

        let mut to_get = Vec::new();
        let mut to_update = Vec::new();

        for (trade, trailing_stoploss) in self.running().values() {
            if trade.was_closed_on_range(range_min, range_max) {
                to_get.push(trade.id());
                continue;
            }

            if let Some(trailing_stoploss) = trailing_stoploss {
                if let Some(new_stoploss) = trade
                    .eval_new_stoploss_on_range(range_min, range_max, *trailing_stoploss)
                    .map_err(|e| LiveError::Generic(e.to_string()))?
                {
                    to_update.push((trade.id(), new_stoploss));
                }
            }
        }

        let mut updated_trades = HashMap::new();
        let mut close_results = Vec::new();

        for chunk in to_update.chunks(1) {
            let update_futures = chunk
                .iter()
                .map(|&(trade_id, new_stoploss)| {
                    api.rest
                        .futures
                        .update_trade_stoploss(trade_id, new_stoploss)
                })
                .collect::<Vec<_>>();

            let update_results = future::join_all(update_futures).await;

            let mut close_futures = Vec::new();

            for (&(trade_id, _), update_res) in chunk.iter().zip(update_results) {
                match update_res {
                    Ok(updated_trade) => {
                        updated_trades.insert(updated_trade.id(), updated_trade);
                    }
                    Err(_) => {
                        close_futures.push(api.rest.futures.close_trade(trade_id));
                    }
                }
            }

            if close_futures.is_empty() {
                continue;
            }

            let new_close_results = future::join_all(close_futures).await;
            close_results.extend(new_close_results);
        }

        let mut closed_trades = close_results
            .into_iter()
            .collect::<result::Result<Vec<_>, _>>()
            .map_err(LiveError::RestApi)?;

        for chunk in to_get.chunks(1) {
            let get_futures = chunk
                .iter()
                .map(|&trade_id| api.rest.futures.get_trade(trade_id))
                .collect::<Vec<_>>();

            let new_closed_trades = future::join_all(get_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)?
                .into_iter()
                .filter_map(|trade| if trade.closed() { Some(trade) } else { None })
                .collect::<Vec<_>>();

            closed_trades.extend(new_closed_trades);
        }

        if updated_trades.is_empty() && closed_trades.is_empty() {
            return Ok(false);
        }

        if !updated_trades.is_empty() {
            self.update_running_trades(updated_trades)?;
        }

        if !closed_trades.is_empty() {
            self.close_trades(closed_trades)?;
        }

        Ok(true)
    }

    pub fn register_running_trade(
        &mut self,
        new_trade: LnmTrade,
        trailing_stoploss: Option<BoundedPercentage>,
    ) -> LiveResult<()> {
        if !new_trade.running() {
            return Err(LiveError::Generic(format!(
                "`new_trade` {} is not running",
                new_trade.id(),
            )));
        }

        if self.running.contains_key(&new_trade.id()) {
            return Err(LiveError::Generic(format!(
                "`new_trade` {} already registered",
                new_trade.id(),
            )));
        }

        if self
            .last_trade_time
            .map_or(true, |last| new_trade.creation_ts() > last)
        {
            self.last_trade_time = Some(new_trade.creation_ts());
        }

        self.balance = {
            self.balance as i64
                - new_trade.margin().into_i64()
                - new_trade.maintenance_margin()
                - new_trade.opening_fee() as i64
        }
        .max(0) as u64;

        self.trigger
            .update(&new_trade, trailing_stoploss)
            .map_err(|e| LiveError::Generic(e.to_string()))?;
        self.running
            .insert(new_trade.id(), (new_trade, trailing_stoploss));

        Ok(())
    }

    pub fn update_running_trades(
        &mut self,
        mut updated_trades: HashMap<Uuid, LnmTrade>,
    ) -> LiveResult<()> {
        if updated_trades.is_empty() {
            return Err(LiveError::Generic(format!("`updated_trades` is empty",)));
        }

        let mut new_running = HashMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;

        for (id, (curr_trade, trailing_stoploss)) in &self.running {
            let running_trade = if let Some(updated_trade) = updated_trades.remove(id) {
                new_balance += curr_trade.margin().into_i64() + curr_trade.maintenance_margin()
                    - updated_trade.margin().into_i64()
                    - updated_trade.maintenance_margin();

                updated_trade
            } else {
                curr_trade.clone()
            };

            // TODO: Improve error handling here
            new_trigger
                .update(&running_trade, *trailing_stoploss)
                .map_err(|e| LiveError::Generic(e.to_string()))?;

            new_running.insert(*id, (running_trade, *trailing_stoploss));
        }

        if !updated_trades.is_empty() {
            let remaining_updated_keys: String = updated_trades
                .into_keys()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");

            return Err(LiveError::Generic(format!(
                "`updated_trade`s {remaining_updated_keys} were not running",
            )))
            .into();
        }

        self.trigger = new_trigger;
        self.running = new_running;
        self.balance = new_balance.max(0) as u64;

        Ok(())
    }

    pub fn close_trades(&mut self, closed_trades: Vec<LnmTrade>) -> LiveResult<()> {
        if closed_trades.is_empty() {
            return Err(LiveError::Generic(format!("`closed_trades` is empty",)));
        }

        let mut closed_map = HashMap::new();
        let mut new_last_trade_time: Option<DateTime<Utc>> = None;

        for closed_trade in closed_trades {
            let closed_ts = closed_trade.closed_ts().ok_or_else(|| {
                LiveError::Generic(format!(
                    "`closed_trade` {} is not closed",
                    closed_trade.id(),
                ))
            })?;

            if !self.running.contains_key(&closed_trade.id()) {
                return Err(LiveError::Generic(format!(
                    "`closed_trade` {} was not running",
                    closed_trade.id(),
                ))
                .into());
            }

            if new_last_trade_time.map_or(true, |last| closed_ts > last) {
                new_last_trade_time = Some(closed_ts);
            }

            closed_map.insert(closed_trade.id(), closed_trade);
        }

        let mut new_running = HashMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;

        for (id, (trade, trailing_stoploss)) in &self.running {
            if let Some(closed_trade) = closed_map.remove(id) {
                new_balance += trade.margin().into_i64() + trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                self.closed.push(closed_trade);
            } else {
                // TODO: Improve error handling here
                new_trigger
                    .update(trade, *trailing_stoploss)
                    .map_err(|e| LiveError::Generic(e.to_string()))?;
                new_running.insert(*id, (trade.clone(), *trailing_stoploss));
            }
        }

        self.last_trade_time = new_last_trade_time;
        self.trigger = new_trigger;
        self.running = new_running;
        self.balance = new_balance.max(0) as u64;

        Ok(())
    }
}

#[derive(Debug)]
pub enum LiveTradeControllerState {
    Starting,
    WaitingForSync(Arc<SyncState>),
    Ready(LiveTradeControllerReadyStatus),
    Failed(LiveError),
    NotViable(LiveError),
}

pub struct LockedLiveTradeControllerReadyStatus<'a> {
    state_guard: MutexGuard<'a, Arc<LiveTradeControllerState>>,
}

impl<'a> TryFrom<MutexGuard<'a, Arc<LiveTradeControllerState>>>
    for LockedLiveTradeControllerReadyStatus<'a>
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

impl<'a> LockedLiveTradeControllerReadyStatus<'a> {
    fn as_status(&self) -> &LiveTradeControllerReadyStatus {
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

    pub fn running(&self) -> &HashMap<Uuid, (LnmTrade, Option<BoundedPercentage>)> {
        &self.as_status().running
    }

    pub fn closed(&self) -> &Vec<LnmTrade> {
        &self.as_status().closed
    }

    pub fn to_owned(&self) -> LiveTradeControllerReadyStatus {
        self.as_status().clone()
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

    pub async fn try_lock_ready_status(&self) -> LiveResult<LockedLiveTradeControllerReadyStatus> {
        let state_guard = self.state.lock().await;
        LockedLiveTradeControllerReadyStatus::try_from(state_guard)
    }

    pub async fn snapshot(&self) -> Arc<LiveTradeControllerState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveTradeControllerReceiver {
        self.state_tx.subscribe()
    }

    fn update_state_guard(
        &self,
        mut state_guard: MutexGuard<'_, Arc<LiveTradeControllerState>>,
        new_state: LiveTradeControllerState,
    ) {
        let new_state = Arc::new(new_state);

        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: LiveTradeControllerState) {
        let state_guard = self.state.lock().await;

        self.update_state_guard(state_guard, new_state)
    }

    pub async fn update_from_locked_ready_status(
        &self,
        locked_ready_status: LockedLiveTradeControllerReadyStatus<'_>,
        new_state: LiveTradeControllerState,
    ) {
        self.update_state_guard(locked_ready_status.state_guard, new_state)
    }
}
