use std::{collections::HashMap, result, sync::Arc};

use chrono::{DateTime, Utc};
use futures::future;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use lnm_sdk::api::rest::models::{BoundedPercentage, LnmTrade, Price, Trade, TradeSide};

use crate::{
    db::DbContext,
    sync::SyncState,
    trade::core::{PriceTrigger, TradeExt, TradeTrailingStoploss, TradingState},
};

use super::super::error::{LiveError, Result as LiveResult};
use super::super::executor::{
    LiveTradeExecutorTransmiter, LiveTradeExecutorUpdate, WrappedApiContext,
};

#[derive(Debug, Clone)]
pub struct LiveTradeExecutorReadyStatus {
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    last_evaluation_time: DateTime<Utc>,
    last_price: f64,
    trigger: PriceTrigger,
    running: HashMap<Uuid, (Arc<LnmTrade>, Option<TradeTrailingStoploss>)>,
    closed: Vec<Arc<LnmTrade>>,
}

impl LiveTradeExecutorReadyStatus {
    pub async fn new(
        tsl_step_size: BoundedPercentage,
        db: &DbContext,
        api: &WrappedApiContext,
    ) -> LiveResult<Self> {
        let (lastest_entry_time, lastest_entry_price) = db
            .price_ticks
            .get_latest_entry()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        let mut registered_trades = db
            .running_trades
            .load_and_validate_trades(tsl_step_size)
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let (running_trades, user) = futures::try_join!(api.get_trades_running(), api.get_user())?;

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

            // Assume that trades that are not registered don't have trailing stoplosses
            let trade_tsl_opt = registered_trades.remove(&trade.id()).and_then(|sl| sl);

            trigger
                .update(tsl_step_size, &trade, trade_tsl_opt)
                .map_err(|e| LiveError::Generic(e.to_string()))?;

            running.insert(trade.id(), (Arc::new(trade), trade_tsl_opt));
        }

        if !registered_trades.is_empty() {
            // Assume that trades remaining at `registered_trades` are
            // outdated and can be removed.

            let trade_uuids: Vec<Uuid> = registered_trades.keys().cloned().collect();
            db.running_trades
                .remove_trades(&trade_uuids)
                .await
                .map_err(|e| LiveError::Generic(e.to_string()))?;
        }

        Ok(Self {
            last_trade_time,
            balance: user.balance(),
            last_evaluation_time: lastest_entry_time,
            last_price: lastest_entry_price,
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

    pub fn running(&self) -> &HashMap<Uuid, (Arc<LnmTrade>, Option<TradeTrailingStoploss>)> {
        &self.running
    }

    pub fn closed(&self) -> &Vec<Arc<LnmTrade>> {
        &self.closed
    }

    pub async fn reevaluate(
        &mut self,
        tsl_step_size: BoundedPercentage,
        db: &DbContext,
        api: &WrappedApiContext,
    ) -> LiveResult<()> {
        let (range_min, range_max, lastest_entry_time, latest_entry_price) = db
            .price_ticks
            .get_price_range_from(self.last_evaluation_time)
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        self.last_evaluation_time = lastest_entry_time;
        self.last_price = latest_entry_price;

        if !self.trigger.was_reached(range_min) && !self.trigger.was_reached(range_max) {
            // General trigger was not reached. No trades need to be checked

            return Ok(());
        }

        let mut to_get = Vec::new();
        let mut to_update = Vec::new();

        for (trade, trade_tsl_opt) in self.running().values() {
            if trade.was_closed_on_range(range_min, range_max) {
                to_get.push(trade.id());
                continue;
            }

            if let Some(trade_tsl) = trade_tsl_opt {
                let new_stoploss_opt = trade
                    .eval_new_stoploss_on_range(tsl_step_size, *trade_tsl, range_min, range_max)
                    .map_err(|e| LiveError::Generic(e.to_string()))?;

                if let Some(new_stoploss) = new_stoploss_opt {
                    to_update.push((trade.id(), new_stoploss));
                }
            }
        }

        let mut updated_trades = HashMap::new();
        let mut close_results = Vec::new();

        for chunk in to_update.chunks(1) {
            let update_futures = chunk
                .iter()
                .map(|&(trade_id, new_stoploss)| api.update_trade_stoploss(trade_id, new_stoploss))
                .collect::<Vec<_>>();

            let update_results = future::join_all(update_futures).await;

            let mut close_futures = Vec::new();

            for (&(trade_id, _), update_res) in chunk.iter().zip(update_results) {
                match update_res {
                    Ok(updated_trade) => {
                        updated_trades.insert(updated_trade.id(), updated_trade);
                    }
                    Err(_) => {
                        close_futures.push(api.close_trade(trade_id));
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
            .collect::<result::Result<Vec<_>, _>>()?;

        for chunk in to_get.chunks(1) {
            let get_futures = chunk
                .iter()
                .map(|&trade_id| api.get_trade(trade_id))
                .collect::<Vec<_>>();

            let new_closed_trades = future::join_all(get_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()?
                .into_iter()
                .filter_map(|trade| if trade.closed() { Some(trade) } else { None })
                .collect::<Vec<_>>();

            closed_trades.extend(new_closed_trades);
        }

        self.update_running_trades(tsl_step_size, updated_trades)?;

        self.close_trades(tsl_step_size, closed_trades)?;

        Ok(())
    }

    pub fn register_running_trade(
        &mut self,
        tsl_step_size: BoundedPercentage,
        new_trade: LnmTrade,
        trade_tsl: Option<TradeTrailingStoploss>,
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
            .update(tsl_step_size, &new_trade, trade_tsl)
            .map_err(|e| LiveError::Generic(e.to_string()))?;
        self.running
            .insert(new_trade.id(), (Arc::new(new_trade), trade_tsl));

        Ok(())
    }

    pub fn update_running_trades(
        &mut self,
        tsl_step_size: BoundedPercentage,
        mut updated_trades: HashMap<Uuid, LnmTrade>,
    ) -> LiveResult<()> {
        if updated_trades.is_empty() {
            return Ok(());
        }

        let mut new_running = HashMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;

        for (id, (curr_trade, trade_tsl)) in &self.running {
            let running_trade = if let Some(updated_trade) = updated_trades.remove(id) {
                new_balance += curr_trade.margin().into_i64() + curr_trade.maintenance_margin()
                    - updated_trade.margin().into_i64()
                    - updated_trade.maintenance_margin();

                Arc::new(updated_trade)
            } else {
                curr_trade.clone()
            };

            // TODO: Improve error handling here
            new_trigger
                .update(tsl_step_size, running_trade.as_ref(), *trade_tsl)
                .map_err(|e| LiveError::Generic(e.to_string()))?;

            new_running.insert(*id, (running_trade, *trade_tsl));
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

    pub fn close_trades(
        &mut self,
        tsl_step_size: BoundedPercentage,
        closed_trades: Vec<LnmTrade>,
    ) -> LiveResult<()> {
        if closed_trades.is_empty() {
            return Ok(());
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

        for (id, (trade, trade_tsl)) in &self.running {
            if let Some(closed_trade) = closed_map.remove(id) {
                new_balance += trade.margin().into_i64() + trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                self.closed.push(Arc::new(closed_trade));
                continue;
            }

            // TODO: Improve error handling here
            new_trigger
                .update(tsl_step_size, trade.as_ref(), *trade_tsl)
                .map_err(|e| LiveError::Generic(e.to_string()))?;
            new_running.insert(*id, (trade.clone(), *trade_tsl));
        }

        self.last_trade_time = new_last_trade_time;
        self.trigger = new_trigger;
        self.running = new_running;
        self.balance = new_balance.max(0) as u64;

        Ok(())
    }
}

impl From<&LiveTradeExecutorReadyStatus> for TradingState {
    fn from(value: &LiveTradeExecutorReadyStatus) -> Self {
        let mut running: Vec<Arc<dyn Trade>> = Vec::new();
        let mut running_long_len: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_long_quantity: u64 = 0;
        let mut running_short_len: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_short_quantity: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;

        for (trade, _) in value.running().values() {
            match trade.side() {
                TradeSide::Buy => {
                    running_long_len += 1;
                    running_long_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_long_quantity += trade.quantity().into_u64();
                }
                TradeSide::Sell => {
                    running_short_len += 1;
                    running_short_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_short_quantity += trade.quantity().into_u64();
                }
            };

            running_pl += trade.estimate_pl(Price::clamp_from(value.last_price));
            running_fees += trade.opening_fee();
            running.push(trade.clone());
        }

        let mut closed_pl: i64 = 0;
        let mut closed_fees: u64 = 0;

        for trade in value.closed() {
            closed_pl += trade.pl();
            closed_fees += trade.opening_fee() + trade.closing_fee();
        }

        let closed = value
            .closed
            .iter()
            .map(|trade| trade.clone() as Arc<dyn Trade>)
            .collect();

        TradingState::new(
            Utc::now(),
            value.balance(),
            value.last_price,
            value.last_trade_time(),
            running,
            running_long_len,
            running_long_margin,
            running_long_quantity,
            running_short_len,
            running_short_margin,
            running_short_quantity,
            running_pl,
            running_fees,
            closed,
            value.closed().len(),
            closed_pl,
            closed_fees,
        )
        .expect("`LiveTradeExecutorReadyStatus` can't contain inconsistent trades")
    }
}

#[derive(Debug)]
pub enum LiveTradeExecutorStateNotReady {
    Starting,
    WaitingForSync(Arc<SyncState>),
    Failed(LiveError),
    NotViable(LiveError),
}

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorState {
    NotReady(Arc<LiveTradeExecutorStateNotReady>),
    Ready(Arc<LiveTradeExecutorReadyStatus>),
}

impl From<LiveTradeExecutorStateNotReady> for LiveTradeExecutorState {
    fn from(value: LiveTradeExecutorStateNotReady) -> Self {
        Self::NotReady(Arc::new(value))
    }
}

impl From<LiveTradeExecutorReadyStatus> for LiveTradeExecutorState {
    fn from(value: LiveTradeExecutorReadyStatus) -> Self {
        Self::Ready(Arc::new(value))
    }
}

impl From<LiveTradeExecutorState> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradeExecutorState) -> Self {
        match value {
            LiveTradeExecutorState::NotReady(not_ready) => Self::NotReady(not_ready),
            LiveTradeExecutorState::Ready(ready_status) => Self::Ready(ready_status.into()),
        }
    }
}

pub struct LockedLiveTradeExecutorReadyStatus<'a> {
    state_guard: MutexGuard<'a, LiveTradeExecutorState>,
}

impl<'a> TryFrom<MutexGuard<'a, LiveTradeExecutorState>>
    for LockedLiveTradeExecutorReadyStatus<'a>
{
    type Error = LiveError;

    fn try_from(
        value: MutexGuard<'a, LiveTradeExecutorState>,
    ) -> result::Result<Self, Self::Error> {
        match *value {
            LiveTradeExecutorState::Ready(_) => Ok(Self { state_guard: value }),
            _ => Err(LiveError::ManagerNotReady),
        }
    }
}

impl<'a> LockedLiveTradeExecutorReadyStatus<'a> {
    fn as_status(&self) -> &LiveTradeExecutorReadyStatus {
        match *self.state_guard {
            LiveTradeExecutorState::Ready(ref status) => status,
            _ => panic!("state must be ready"),
        }
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.as_status().last_trade_time
    }

    pub fn balance(&self) -> u64 {
        self.as_status().balance
    }

    pub fn running(&self) -> &HashMap<Uuid, (Arc<LnmTrade>, Option<TradeTrailingStoploss>)> {
        &self.as_status().running
    }

    pub fn closed(&self) -> &Vec<Arc<LnmTrade>> {
        &self.as_status().closed
    }

    pub fn to_owned(&self) -> LiveTradeExecutorReadyStatus {
        self.as_status().clone()
    }
}

pub struct LiveTradeExecutorStateManager {
    state: Mutex<LiveTradeExecutorState>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl LiveTradeExecutorStateManager {
    pub fn new(update_tx: LiveTradeExecutorTransmiter) -> Arc<Self> {
        let initial_state =
            LiveTradeExecutorState::NotReady(Arc::new(LiveTradeExecutorStateNotReady::Starting));
        let state = Mutex::new(initial_state);

        Arc::new(Self { state, update_tx })
    }

    pub async fn try_lock_ready_status(&self) -> LiveResult<LockedLiveTradeExecutorReadyStatus> {
        let state_guard = self.state.lock().await;
        LockedLiveTradeExecutorReadyStatus::try_from(state_guard)
    }

    pub async fn snapshot(&self) -> LiveTradeExecutorState {
        self.state.lock().await.clone()
    }

    fn update_state_guard(
        &self,
        mut state_guard: MutexGuard<'_, LiveTradeExecutorState>,
        new_state: LiveTradeExecutorState,
    ) {
        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_state.into());
    }

    pub async fn update(&self, new_state: LiveTradeExecutorState) {
        let state_guard = self.state.lock().await;

        self.update_state_guard(state_guard, new_state.into())
    }

    pub async fn update_from_locked_ready_status(
        &self,
        locked_ready_status: LockedLiveTradeExecutorReadyStatus<'_>,
        new_state: LiveTradeExecutorState,
    ) {
        self.update_state_guard(locked_ready_status.state_guard, new_state)
    }
}
