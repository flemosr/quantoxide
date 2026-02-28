use std::{
    collections::{HashMap, HashSet},
    result, slice,
    sync::Arc,
};

use chrono::{DateTime, Duration, Timelike, Utc};
use futures::future;
use uuid::Uuid;

use lnm_sdk::api_v3::models::{PercentageCapped, Price, Trade};

use crate::{db::Database, util::DateTimeExt};

use super::super::super::{
    super::core::{
        ClosedTradeHistory, DynRunningTradesMap, PriceTrigger, RunningTradesMap, TradeRunningExt,
        TradeTrailingStoploss, TradingState,
    },
    executor::{
        WrappedRestClient,
        error::{ExecutorActionError, ExecutorActionResult},
    },
};

/// Sessions expire 5 minutes past each hour to ensure funding fees settled at the rounded hour are
/// reflected in the API state before renewal.
const SESSION_EXPIRY_OFFSET_MIN: u32 = 5;

#[derive(Debug, Clone)]
pub(in crate::trade) struct LiveTradingSession {
    expires_at: DateTime<Utc>,
    tsl_step_size: PercentageCapped,
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    last_evaluation_time: DateTime<Utc>,
    last_price: f64,
    trigger: PriceTrigger,
    running_map: DynRunningTradesMap,
    realized_pl: i64,
    closed_history: Arc<ClosedTradeHistory>,
    closed_fees: u64,
    funding_fees: i64,
    funding_snapshot: HashMap<Uuid, i64>,
}

impl LiveTradingSession {
    pub async fn new(
        recover_trades_on_startup: bool,
        tsl_step_size: PercentageCapped,
        db: &Database,
        api: &WrappedRestClient,
        prev_trading_session: Option<Self>,
    ) -> ExecutorActionResult<Self> {
        let (lastest_entry_time, lastest_entry_price) = db
            .price_ticks
            .get_latest_entry()
            .await?
            .ok_or(ExecutorActionError::DbIsEmpty)?;

        let user = api.get_user().await?;

        let now = Utc::now();
        let expires_at = if now.minute() < SESSION_EXPIRY_OFFSET_MIN {
            now.floor_hour() + Duration::minutes(SESSION_EXPIRY_OFFSET_MIN as i64)
        } else {
            now.floor_hour()
                + Duration::hours(1)
                + Duration::minutes(SESSION_EXPIRY_OFFSET_MIN as i64)
        };

        let (prev_funding_fees, prev_funding_snapshot) = prev_trading_session
            .as_ref()
            .map_or((0, HashMap::new()), |ps| {
                (ps.funding_fees, ps.funding_snapshot.clone())
            });

        let mut session = Self {
            expires_at,
            tsl_step_size,
            last_trade_time: None,
            balance: user.balance(),
            last_evaluation_time: lastest_entry_time,
            last_price: lastest_entry_price,
            trigger: PriceTrigger::NotSet,
            running_map: RunningTradesMap::new(),
            realized_pl: prev_trading_session.as_ref().map_or(0, |ps| ps.realized_pl),
            closed_history: prev_trading_session.as_ref().map_or_else(
                || Arc::new(ClosedTradeHistory::new()),
                |ps| ps.closed_history.clone(),
            ),
            closed_fees: prev_trading_session.as_ref().map_or(0, |ps| ps.closed_fees),
            funding_fees: prev_funding_fees,
            funding_snapshot: HashMap::new(),
        };

        if !recover_trades_on_startup {
            return Ok(session);
        }

        let running_trades = api.get_trades_running().await?;

        // Reconcile trades that closed between the last `reevaluate()` and the fresh API
        // state fetched above. Once the session expires, `reevaluate()` stops running, so any
        // closure during that window is missed.
        if let Some(prev_session) = &prev_trading_session {
            let running_ids: HashSet<Uuid> = running_trades.iter().map(|t| t.id()).collect();

            let missed_ids: HashSet<Uuid> = prev_session
                .running_map
                .trades_desc()
                .map(|(t, _)| t.id())
                .filter(|id| !running_ids.contains(id))
                .collect();

            if !missed_ids.is_empty() {
                let limit = (prev_session.running_map.len() as u64)
                    .max(1)
                    .try_into()
                    .expect("valid `NonZeroU64`");

                let closed_trades = api.get_trades_closed(limit).await?;

                let mut unconfirmed = missed_ids.clone();

                let closed_history = Arc::make_mut(&mut session.closed_history);

                for closed_trade in &closed_trades {
                    if !missed_ids.contains(&closed_trade.id()) {
                        continue;
                    }

                    unconfirmed.remove(&closed_trade.id());

                    session.realized_pl += closed_trade.pl();
                    session.closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();

                    closed_history
                        .add(Arc::new(closed_trade.clone()))
                        .map_err(ExecutorActionError::ClosedHistoryUpdate)?;

                    let baseline = prev_funding_snapshot
                        .get(&closed_trade.id())
                        .copied()
                        .unwrap_or(0);
                    session.funding_fees += closed_trade.sum_funding_fees() - baseline;

                    let closed_at = closed_trade.closed_at();
                    if let Some(closed_at) = closed_at
                        && session.last_trade_time.is_none_or(|last| closed_at > last)
                    {
                        session.last_trade_time = Some(closed_at);
                    }
                }

                if let Some(&trade_id) = unconfirmed.iter().next() {
                    return Err(ExecutorActionError::ClosedTradeNotConfirmed { trade_id });
                }
            }
        }

        // Try to recover trades 'trailing stoploss' config from db

        let mut registered_trades_map = db.running_trades.get_running_trades_map().await?;

        for trade in running_trades {
            let trade_tsl = registered_trades_map.remove(&trade.id()).flatten();

            // Subtract the previous baseline so that `register_running_trade`'s realization of
            // `sum_funding_fees` yields the correct delta.
            let baseline = prev_funding_snapshot
                .get(&trade.id())
                .copied()
                .unwrap_or(trade.sum_funding_fees());
            session.funding_fees -= baseline;

            // Balance obtained via API is up-to-date
            session.register_running_trade(trade, trade_tsl, false)?;
        }

        if !registered_trades_map.is_empty() {
            // Trades still on the map are not running

            let dangling_registered_trades: Vec<Uuid> =
                registered_trades_map.keys().cloned().collect();

            db.running_trades
                .remove_running_trades(dangling_registered_trades.as_slice())
                .await?;
        }

        Ok(session)
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn running_map(&self) -> &DynRunningTradesMap {
        &self.running_map
    }

    pub async fn reevaluate(
        &mut self,
        db: &Database,
        api: &WrappedRestClient,
    ) -> ExecutorActionResult<Vec<Trade>> {
        let (range_min, range_max, lastest_entry_time, latest_entry_price) = db
            .price_ticks
            .get_price_range_from(self.last_evaluation_time)
            .await?
            .ok_or(ExecutorActionError::DbIsEmpty)?;

        self.last_evaluation_time = lastest_entry_time;
        self.last_price = latest_entry_price;

        if !self.trigger.was_reached(range_min) && !self.trigger.was_reached(range_max) {
            // General trigger was not reached. No trades need to be checked
            return Ok(Vec::new());
        }

        let mut to_confirm_closed = HashSet::new();
        let mut to_update = Vec::new();

        for (trade, trade_tsl_opt) in self.running_map().trades_desc() {
            if trade.was_closed_on_range(range_min, range_max) {
                to_confirm_closed.insert(trade.id());
                continue;
            }

            if let Some(trade_tsl) = trade_tsl_opt {
                let new_stoploss_opt = trade
                    .eval_new_stoploss_on_range(
                        self.tsl_step_size,
                        *trade_tsl,
                        range_min,
                        range_max,
                    )
                    .map_err(ExecutorActionError::StoplossEvaluation)?;

                if let Some(new_stoploss) = new_stoploss_opt {
                    to_update.push((trade.id(), new_stoploss));
                }
            }
        }

        let mut closed_trades = Vec::new();

        if !to_confirm_closed.is_empty() {
            let limit = (to_confirm_closed.len() as u64)
                .try_into()
                .expect("valid `NonZeroU64`");
            let recently_closed_trades = api.get_trades_closed(limit).await?;

            for closed_trade in &recently_closed_trades {
                let trade_id = closed_trade.id();

                if !to_confirm_closed.remove(&trade_id) {
                    return Err(ExecutorActionError::UnexpectedClosedTrade { trade_id });
                }
            }

            if !to_confirm_closed.is_empty() {
                let trade_id = to_confirm_closed.drain().next().expect("not empty");
                return Err(ExecutorActionError::ClosedTradeNotConfirmed { trade_id });
            }

            closed_trades.extend(recently_closed_trades);
        }

        let mut updated_trades = HashMap::new();
        let mut close_results = Vec::new();

        for chunk in to_update.chunks(3) {
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

        let new_closed_trades = close_results
            .into_iter()
            .collect::<result::Result<Vec<_>, _>>()?;

        closed_trades.extend(new_closed_trades);

        self.update_running_trades(updated_trades)?;

        self.close_trades(&closed_trades)?;

        Ok(closed_trades)
    }

    pub fn register_running_trade(
        &mut self,
        new_trade: Trade,
        trade_tsl: Option<TradeTrailingStoploss>,
        update_balance: bool,
    ) -> ExecutorActionResult<()> {
        if !new_trade.running() {
            return Err(ExecutorActionError::NewTradeNotRunning {
                trade_id: new_trade.id(),
            });
        }

        if self.running_map.contains(&new_trade.id()) {
            return Err(ExecutorActionError::TradeAlreadyRegistered {
                trade_id: new_trade.id(),
            });
        }

        if self
            .last_trade_time
            .is_none_or(|last| new_trade.created_at() > last)
        {
            self.last_trade_time = Some(new_trade.created_at());
        }

        if update_balance {
            self.balance = self
                .balance
                .saturating_sub(new_trade.margin().as_u64())
                .saturating_sub(new_trade.maintenance_margin() as u64)
                .saturating_sub(new_trade.opening_fee());
        }

        self.trigger
            .update(self.tsl_step_size, &new_trade, trade_tsl)
            .map_err(ExecutorActionError::PriceTriggerUpdate)?;

        // Realize `sum_funding_fees` now and record it as the baseline for future deltas. This
        // handles the edge case where a trade is created right at a funding settlement and already
        // carries non-zero fees by the time we see it.
        self.funding_fees += new_trade.sum_funding_fees();
        self.funding_snapshot
            .insert(new_trade.id(), new_trade.sum_funding_fees());

        self.running_map.add(Arc::new(new_trade), trade_tsl);

        Ok(())
    }

    pub fn update_running_trades(
        &mut self,
        mut updated_trades: HashMap<Uuid, Trade>,
    ) -> ExecutorActionResult<()> {
        if updated_trades.is_empty() {
            return Ok(());
        }

        let mut new_running_map = RunningTradesMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;
        let mut new_realized_pl = self.realized_pl;

        for (curr_trade, trade_tsl) in self.running_map.trades_desc() {
            let running_trade = if let Some(updated_trade) = updated_trades.remove(&curr_trade.id())
            {
                // As of Dec 10 2025, using `.round` here seems to match LNM's behavior.
                let cashed_in_pl = curr_trade.est_pl(updated_trade.price()).round() as i64;

                let collateral_delta =
                    curr_trade.margin().as_i64() + curr_trade.maintenance_margin() + cashed_in_pl
                        - updated_trade.margin().as_i64()
                        - updated_trade.maintenance_margin();

                new_balance += collateral_delta;
                new_realized_pl += cashed_in_pl;

                Arc::new(updated_trade)
            } else {
                curr_trade.clone()
            };

            new_trigger
                .update(self.tsl_step_size, running_trade.as_ref(), *trade_tsl)
                .map_err(ExecutorActionError::PriceTriggerUpdate)?;

            new_running_map.add(running_trade, *trade_tsl);
        }

        if !updated_trades.is_empty() {
            let trade_ids: Vec<Uuid> = updated_trades.into_keys().collect::<Vec<_>>();

            return Err(ExecutorActionError::UpdatedTradesNotRunning { trade_ids });
        }

        self.trigger = new_trigger;
        self.running_map = new_running_map;
        self.balance = new_balance.max(0) as u64;
        self.realized_pl = new_realized_pl;

        Ok(())
    }

    pub fn update_running_trade(&mut self, updated_trade: Trade) -> ExecutorActionResult<()> {
        let mut updated_trades_map = HashMap::new();
        updated_trades_map.insert(updated_trade.id(), updated_trade);
        self.update_running_trades(updated_trades_map)
    }

    pub fn close_trades(&mut self, closed_trades: &[Trade]) -> ExecutorActionResult<()> {
        if closed_trades.is_empty() {
            return Ok(());
        }

        let mut closed_map = HashMap::new();
        let mut new_last_trade_time: Option<DateTime<Utc>> = None;

        for closed_trade in closed_trades {
            let closed_at =
                closed_trade
                    .closed_at()
                    .ok_or_else(|| ExecutorActionError::TradeNotClosed {
                        trade_id: closed_trade.id(),
                    })?;

            if !self.running_map.contains(&closed_trade.id()) {
                return Err(ExecutorActionError::TradeNotRegistered {
                    trade_id: closed_trade.id(),
                });
            }

            if new_last_trade_time.is_none_or(|last| closed_at > last) {
                new_last_trade_time = Some(closed_at);
            }

            closed_map.insert(closed_trade.id(), closed_trade);
        }

        let mut new_running_map = RunningTradesMap::new();
        let mut new_trigger = PriceTrigger::NotSet;
        let mut new_balance = self.balance as i64;
        let mut new_realized_pl = self.realized_pl;
        let mut new_closed_fees = self.closed_fees;
        let closed_history = Arc::make_mut(&mut self.closed_history);

        for (runnning_trade, trade_tsl) in self.running_map.trades_desc() {
            if let Some(closed_trade) = closed_map.remove(&runnning_trade.id()) {
                new_balance += runnning_trade.margin().as_i64()
                    + runnning_trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                new_realized_pl += closed_trade.pl();
                new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();

                if let Some(baseline) = self.funding_snapshot.remove(&closed_trade.id()) {
                    self.funding_fees += closed_trade.sum_funding_fees() - baseline;
                }

                closed_history
                    .add(Arc::new(closed_trade.clone()))
                    .map_err(ExecutorActionError::ClosedHistoryUpdate)?;

                continue;
            }

            new_trigger
                .update(self.tsl_step_size, runnning_trade.as_ref(), *trade_tsl)
                .map_err(ExecutorActionError::PriceTriggerUpdate)?;

            new_running_map.add(runnning_trade.clone(), *trade_tsl);
        }

        self.last_trade_time = new_last_trade_time;
        self.trigger = new_trigger;
        self.running_map = new_running_map;
        self.balance = new_balance.max(0) as u64;
        self.realized_pl = new_realized_pl;
        self.closed_fees = new_closed_fees;

        Ok(())
    }

    pub fn close_trade(&mut self, closed_trade: &Trade) -> ExecutorActionResult<()> {
        self.close_trades(slice::from_ref(closed_trade))
    }
}

impl From<LiveTradingSession> for TradingState {
    fn from(value: LiveTradingSession) -> Self {
        TradingState::new(
            value.last_evaluation_time,
            value.balance,
            Price::bounded(value.last_price),
            value.last_trade_time,
            value.running_map,
            value.funding_fees,
            value.realized_pl,
            value.closed_history,
            value.closed_fees,
        )
    }
}
