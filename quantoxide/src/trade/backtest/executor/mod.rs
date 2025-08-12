use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, Price, Trade, TradeClosed, TradeRunning, TradeSide, TradeSize,
};

use super::super::{
    core::{PriceTrigger, RunningTradesMap, Stoploss, TradeExecutor, TradeExt, TradingState},
    error::{Result, TradeError},
};

pub mod error;
mod models;

use error::SimulatedTradeExecutorError;
use models::SimulatedTradeRunning;

enum Close {
    Single(Uuid),
    Side(TradeSide),
    All,
}

impl From<TradeSide> for Close {
    fn from(value: TradeSide) -> Self {
        Self::Side(value)
    }
}

struct SimulatedTradeExecutorState {
    time: DateTime<Utc>,
    last_tick_time: DateTime<Utc>,
    market_price: f64,
    balance: i64,
    last_trade_time: Option<DateTime<Utc>>,
    trigger: PriceTrigger,
    running_map: RunningTradesMap<SimulatedTradeRunning>,
    closed_len: usize,
    closed_pl: i64,
    closed_fees: u64,
}

pub struct SimulatedTradeExecutor {
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    tsl_step_size: BoundedPercentage,
    state: Arc<Mutex<SimulatedTradeExecutorState>>,
}

impl SimulatedTradeExecutor {
    pub fn new(
        max_running_qtd: usize,
        fee_perc: BoundedPercentage,
        tsl_step_size: BoundedPercentage,
        start_time: DateTime<Utc>,
        market_price: f64,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradeExecutorState {
            time: start_time,
            last_tick_time: start_time,
            market_price,
            balance: start_balance as i64,
            last_trade_time: None,
            trigger: PriceTrigger::new(),
            running_map: RunningTradesMap::new(),
            closed_len: 0,
            closed_pl: 0,
            closed_fees: 0,
        };

        Self {
            max_running_qtd,
            fee_perc,
            tsl_step_size,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    pub async fn time_update(&self, time: DateTime<Utc>) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        if time < state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            })?;
        }

        state_guard.time = time;
        Ok(())
    }

    pub async fn tick_update(&self, time: DateTime<Utc>, price: f64) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        if time <= state_guard.last_tick_time || time < state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            })?;
        }

        state_guard.time = time;
        state_guard.last_tick_time = time;
        state_guard.market_price = price;

        if !state_guard.trigger.was_reached(price) {
            return Ok(());
        }

        // The market price reached some `stoploss` and/or `takeprofit`. Running
        // trades must be re-evaluated.

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut close_trade = |trade: &SimulatedTradeRunning, close_price: Price| {
            let closed_trade = trade.to_closed(self.fee_perc, time, close_price);

            new_balance += closed_trade.margin().into_i64() + closed_trade.maintenance_margin()
                - closed_trade.closing_fee() as i64
                + closed_trade.pl();

            new_closed_len += 1;
            new_closed_pl += closed_trade.pl();
            new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
        };

        let mut new_trigger = PriceTrigger::new();
        let mut new_running_map = RunningTradesMap::new();

        for (trade, trade_tsl_opt) in state_guard.running_map.trades_desc_mut() {
            // Check if price reached stoploss or takeprofit

            let (trade_min_opt, trade_max_opt) = match trade.side() {
                TradeSide::Buy => (trade.stoploss(), trade.takeprofit()),
                TradeSide::Sell => (trade.takeprofit(), trade.stoploss()),
            };

            if let Some(trade_min) = trade_min_opt {
                if price <= trade_min.into_f64() {
                    close_trade(trade.as_ref(), trade_min);
                    continue;
                }
            }

            if let Some(trade_max) = trade_max_opt {
                if price >= trade_max.into_f64() {
                    close_trade(trade.as_ref(), trade_max);
                    continue;
                }
            }

            if let Some(trade_tsl) = *trade_tsl_opt {
                let next_stoploss_update_trigger =
                    trade.next_stoploss_update_trigger(self.tsl_step_size, trade_tsl)?;

                let market_price = Price::round(price)
                    .map_err(|e| SimulatedTradeExecutorError::Generic(e.to_string()))?;

                let new_stoploss = match trade.side() {
                    TradeSide::Buy if market_price >= next_stoploss_update_trigger => {
                        let new_stoploss = market_price
                            .apply_discount(trade_tsl.into())
                            .map_err(|e| SimulatedTradeExecutorError::Generic(e.to_string()))?;
                        Some(new_stoploss)
                    }
                    TradeSide::Sell if market_price <= next_stoploss_update_trigger => {
                        let new_stoploss = market_price
                            .apply_gain(trade_tsl.into())
                            .map_err(|e| SimulatedTradeExecutorError::Generic(e.to_string()))?;
                        Some(new_stoploss)
                    }
                    _ => None,
                };

                if let Some(new_stoploss) = new_stoploss {
                    *trade = trade.with_new_stoploss(market_price, new_stoploss)?;
                }
            }

            new_trigger.update(self.tsl_step_size, trade.as_ref(), *trade_tsl_opt)?;
            new_running_map.add(trade.clone(), *trade_tsl_opt);
        }

        state_guard.balance = new_balance;

        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.closed_len = new_closed_len;
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn close_running(&self, close: Close) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut close_trade = |trade: Arc<SimulatedTradeRunning>| {
            let closed_trade = trade.to_closed(self.fee_perc, time, market_price);

            new_balance += closed_trade.margin().into_i64() + closed_trade.maintenance_margin()
                - closed_trade.closing_fee() as i64
                + closed_trade.pl();

            new_closed_len += 1;
            new_closed_pl += closed_trade.pl();
            new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
        };

        let mut new_trigger = PriceTrigger::new();
        let mut new_running_map = RunningTradesMap::new();

        for (trade, trade_tsl) in state_guard.running_map.trades_desc() {
            let should_be_closed = match &close {
                Close::Single(id) if *id == trade.id() => true,
                Close::Side(side) if *side == trade.side() => true,
                Close::All => true,
                _ => false,
            };

            if should_be_closed {
                close_trade(trade.clone());
            } else {
                new_trigger.update(self.tsl_step_size, trade.as_ref(), *trade_tsl)?;
                new_running_map.add(trade.clone(), *trade_tsl);
            }
        }

        state_guard.balance = new_balance;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.closed_len = new_closed_len;
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn create_running(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;

        let (stoploss_price, trade_tsl) = match stoploss {
            Some(stoploss) => {
                let (stoploss_price, tsl) =
                    stoploss.evaluate(self.tsl_step_size, side, market_price)?;
                (Some(stoploss_price), tsl)
            }
            None => (None, None),
        };

        let trade = SimulatedTradeRunning::new(
            side,
            size,
            leverage,
            state_guard.time,
            market_price,
            stoploss_price,
            takeprofit,
            self.fee_perc,
        )?;

        let balance_delta = trade.margin().into_i64() + trade.maintenance_margin();
        if balance_delta > state_guard.balance {
            return Err(TradeError::BalanceTooLow);
        }

        if state_guard.running_map.len() >= self.max_running_qtd {
            return Err(SimulatedTradeExecutorError::MaxRunningTradesReached {
                max_qtd: self.max_running_qtd,
            })?;
        }

        state_guard.balance -= trade.margin().into_i64()
            + trade.maintenance_margin() as i64
            + trade.opening_fee() as i64;

        state_guard.last_trade_time = trade.market_filled_ts();

        state_guard
            .trigger
            .update(self.tsl_step_size, trade.as_ref(), trade_tsl)?;
        state_guard.running_map.add(trade, trade_tsl);

        Ok(())
    }
}

#[async_trait]
impl TradeExecutor for SimulatedTradeExecutor {
    async fn open_long(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> Result<()> {
        self.create_running(TradeSide::Buy, size, leverage, stoploss, takeprofit)
            .await
    }

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> Result<()> {
        self.create_running(TradeSide::Sell, size, leverage, stoploss, takeprofit)
            .await
    }

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.balance < amount.get() as i64 {
            return Err(TradeError::Generic("not enough balance".to_string()));
        }

        let Some((trade, _)) = state_guard.running_map.get_trade_by_id_mut(trade_id) else {
            return Err(TradeError::Generic(format!(
                "trade {trade_id} is not running"
            )));
        };

        let updated_trade = trade.with_added_margin(amount)?;

        *trade = updated_trade;
        state_guard.balance -= amount.get() as i64;

        Ok(())
    }

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let market_price = Price::clamp_from(state_guard.market_price);

        let Some((trade, _)) = state_guard.running_map.get_trade_by_id_mut(trade_id) else {
            return Err(TradeError::Generic(format!(
                "trade {trade_id} is not running"
            )));
        };

        let updated_trade = trade.with_cash_in(market_price, amount)?;

        let cashed_in_pl = trade.est_pl(updated_trade.price()).round() as i64;

        *trade = updated_trade;

        state_guard.balance += amount.get() as i64;
        state_guard.closed_pl += cashed_in_pl;

        Ok(())
    }

    async fn close_trade(&self, trade_id: Uuid) -> Result<()> {
        self.close_running(Close::Single(trade_id)).await?;
        Ok(())
    }

    async fn close_longs(&self) -> Result<()> {
        self.close_running(TradeSide::Buy.into()).await?;
        Ok(())
    }

    async fn close_shorts(&self) -> Result<()> {
        self.close_running(TradeSide::Sell.into()).await?;
        Ok(())
    }

    async fn close_all(&self) -> Result<()> {
        self.close_running(Close::All).await?;
        Ok(())
    }

    async fn trading_state(&self) -> Result<TradingState> {
        let state_guard = self.state.lock().await;

        let trades_state = TradingState::new(
            state_guard.last_tick_time,
            state_guard.balance.max(0) as u64,
            Price::clamp_from(state_guard.market_price),
            state_guard.last_trade_time,
            state_guard.running_map.clone().into_dyn(),
            state_guard.closed_len,
            state_guard.closed_pl,
            state_guard.closed_fees,
        );

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests;
