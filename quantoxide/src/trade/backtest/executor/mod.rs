use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use lnm_sdk::api_v3::models::{Leverage, Price, TradeSide, TradeSize};

use crate::db::models::OhlcCandleRow;

use super::{
    super::{
        core::{
            PriceTrigger, RunningTradesMap, Stoploss, TradeClosed, TradeCore, TradeExecutor,
            TradeRunning, TradeRunningExt, TradingState,
        },
        error::TradeExecutorResult,
    },
    config::SimulatedTradeExecutorConfig,
};

pub(crate) mod error;
mod models;

use error::{SimulatedTradeExecutorError, SimulatedTradeExecutorResult};
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
    // TODO: Remove `last_tick_time`
    last_tick_time: DateTime<Utc>,
    market_price: f64,
    balance: i64,
    last_trade_time: Option<DateTime<Utc>>,
    trigger: PriceTrigger,
    running_map: RunningTradesMap<SimulatedTradeRunning>,
    realized_pl: i64,
    closed_len: usize,
    closed_fees: u64,
}

pub(super) struct SimulatedTradeExecutor {
    config: SimulatedTradeExecutorConfig,
    state: Arc<Mutex<SimulatedTradeExecutorState>>,
}

impl SimulatedTradeExecutor {
    pub fn new(
        config: impl Into<SimulatedTradeExecutorConfig>,
        start_candle: &OhlcCandleRow,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradeExecutorState {
            time: start_candle.time + Duration::seconds(59),
            last_tick_time: start_candle.time + Duration::seconds(59),
            market_price: start_candle.close,
            balance: start_balance as i64,
            last_trade_time: None,
            trigger: PriceTrigger::new(),
            running_map: RunningTradesMap::new(),
            realized_pl: 0,
            closed_len: 0,
            closed_fees: 0,
        };

        Self {
            config: config.into(),
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    pub async fn candle_update(&self, candle: &OhlcCandleRow) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let time = candle.time + Duration::seconds(59);

        if time <= state_guard.last_tick_time || time < state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            })?;
        }

        state_guard.time = time;
        state_guard.last_tick_time = time;
        state_guard.market_price = candle.close;

        if !state_guard.trigger.was_reached(candle.low)
            && !state_guard.trigger.was_reached(candle.high)
        {
            return Ok(());
        }

        // The market price reached some `stoploss` and/or `takeprofit`. Running
        // trades must be re-evaluated.

        let mut new_balance = state_guard.balance as i64;
        let mut new_realized_pl = state_guard.realized_pl;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_last_trade_time = state_guard.last_trade_time;

        let mut close_trade = |trade: &SimulatedTradeRunning, close_price: Price| {
            let closed_trade = trade.to_closed(self.config.fee_perc(), candle.time, close_price);

            new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                - closed_trade.closing_fee() as i64
                + closed_trade.pl();

            new_realized_pl += closed_trade.pl();
            new_closed_len += 1;
            new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
            new_last_trade_time = Some(time);
        };

        let mut new_trigger = PriceTrigger::new();
        let mut new_running_map = RunningTradesMap::new();

        for (trade, trade_tsl_opt) in state_guard.running_map.trades_desc_mut() {
            // Check if price reached stoploss or takeprofit

            let (trade_min_opt, trade_max_opt) = match trade.side() {
                TradeSide::Buy => (trade.stoploss(), trade.takeprofit()),
                TradeSide::Sell => (trade.takeprofit(), trade.stoploss()),
            };

            if let Some(trade_min) = trade_min_opt
                && candle.low <= trade_min.as_f64()
            {
                close_trade(trade.as_ref(), trade_min);
                continue;
            }

            if let Some(trade_max) = trade_max_opt
                && candle.high >= trade_max.as_f64()
            {
                close_trade(trade.as_ref(), trade_max);
                continue;
            }

            if let Some(trade_tsl) = *trade_tsl_opt {
                let next_stoploss_update_trigger = trade
                    .next_stoploss_update_trigger(
                        self.config.trailing_stoploss_step_size(),
                        trade_tsl,
                    )
                    .map_err(SimulatedTradeExecutorError::StoplossEvaluation)?;

                let market_price = Price::round(candle.close)
                    .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;

                let new_stoploss = match trade.side() {
                    TradeSide::Buy if market_price >= next_stoploss_update_trigger => {
                        let new_stoploss = market_price
                            .apply_discount(trade_tsl.into())
                            .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;
                        Some(new_stoploss)
                    }
                    TradeSide::Sell if market_price <= next_stoploss_update_trigger => {
                        let new_stoploss = market_price
                            .apply_gain(trade_tsl.into())
                            .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;
                        Some(new_stoploss)
                    }
                    _ => None,
                };

                if let Some(new_stoploss) = new_stoploss {
                    *trade = trade.with_new_stoploss(market_price, new_stoploss)?;
                }
            }

            new_trigger
                .update(
                    self.config.trailing_stoploss_step_size(),
                    trade.as_ref(),
                    *trade_tsl_opt,
                )
                .map_err(SimulatedTradeExecutorError::PriceTriggerUpdate)?;
            new_running_map.add(trade.clone(), *trade_tsl_opt);
        }

        state_guard.balance = new_balance;

        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.realized_pl = new_realized_pl;
        state_guard.closed_len = new_closed_len;
        state_guard.closed_fees = new_closed_fees;
        state_guard.last_trade_time = new_last_trade_time;

        Ok(())
    }

    async fn close_running(&self, close: Close) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;

        let mut new_balance = state_guard.balance as i64;
        let mut new_realized_pl = state_guard.realized_pl;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut close_trade = |trade: Arc<SimulatedTradeRunning>| {
            let closed_trade = trade.to_closed(self.config.fee_perc(), time, market_price);

            new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                - closed_trade.closing_fee() as i64
                + closed_trade.pl();

            new_realized_pl += closed_trade.pl();
            new_closed_len += 1;
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
                new_trigger
                    .update(
                        self.config.trailing_stoploss_step_size(),
                        trade.as_ref(),
                        *trade_tsl,
                    )
                    .map_err(SimulatedTradeExecutorError::PriceTriggerUpdate)?;
                new_running_map.add(trade.clone(), *trade_tsl);
            }
        }

        state_guard.balance = new_balance;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.realized_pl = new_realized_pl;
        state_guard.closed_len = new_closed_len;
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
    ) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;

        let (stoploss_price, trade_tsl) = match stoploss {
            Some(stoploss) => {
                let (stoploss_price, tsl) = stoploss
                    .evaluate(
                        self.config.trailing_stoploss_step_size(),
                        side,
                        market_price,
                    )
                    .map_err(SimulatedTradeExecutorError::StoplossEvaluation)?;
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
            self.config.fee_perc(),
        )?;

        let balance_delta = trade.margin().as_i64() + trade.maintenance_margin();
        if balance_delta > state_guard.balance {
            return Err(SimulatedTradeExecutorError::BalanceTooLow);
        }

        if state_guard.running_map.len() >= self.config.max_running_qtd() {
            return Err(SimulatedTradeExecutorError::MaxRunningTradesReached {
                max_qtd: self.config.max_running_qtd(),
            })?;
        }

        state_guard.balance -= trade.margin().as_i64()
            + trade.maintenance_margin() as i64
            + trade.opening_fee() as i64;

        state_guard.last_trade_time = trade.market_filled_ts();

        state_guard
            .trigger
            .update(
                self.config.trailing_stoploss_step_size(),
                trade.as_ref(),
                trade_tsl,
            )
            .map_err(SimulatedTradeExecutorError::PriceTriggerUpdate)?;
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
    ) -> TradeExecutorResult<()> {
        self.create_running(TradeSide::Buy, size, leverage, stoploss, takeprofit)
            .await?;
        Ok(())
    }

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> TradeExecutorResult<()> {
        self.create_running(TradeSide::Sell, size, leverage, stoploss, takeprofit)
            .await?;
        Ok(())
    }

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.balance < amount.get() as i64 {
            return Err(SimulatedTradeExecutorError::BalanceTooLow)?;
        }

        let Some((trade, _)) = state_guard.running_map.get_trade_by_id_mut(trade_id) else {
            return Err(SimulatedTradeExecutorError::TradeNotRunning { trade_id })?;
        };

        let updated_trade = trade.with_added_margin(amount)?;

        *trade = updated_trade;
        state_guard.balance -= amount.get() as i64;

        Ok(())
    }

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let market_price = Price::bounded(state_guard.market_price);

        let Some((trade, _)) = state_guard.running_map.get_trade_by_id_mut(trade_id) else {
            return Err(SimulatedTradeExecutorError::TradeNotRunning { trade_id })?;
        };

        let updated_trade = trade.with_cash_in(market_price, amount)?;

        let cashed_in_pl = trade.est_pl(updated_trade.price()).round() as i64;

        *trade = updated_trade;

        state_guard.balance += amount.get() as i64;
        state_guard.realized_pl += cashed_in_pl;

        Ok(())
    }

    async fn close_trade(&self, trade_id: Uuid) -> TradeExecutorResult<()> {
        self.close_running(Close::Single(trade_id)).await?;
        Ok(())
    }

    async fn close_longs(&self) -> TradeExecutorResult<()> {
        self.close_running(TradeSide::Buy.into()).await?;
        Ok(())
    }

    async fn close_shorts(&self) -> TradeExecutorResult<()> {
        self.close_running(TradeSide::Sell.into()).await?;
        Ok(())
    }

    async fn close_all(&self) -> TradeExecutorResult<()> {
        self.close_running(Close::All).await?;
        Ok(())
    }

    async fn trading_state(&self) -> TradeExecutorResult<TradingState> {
        let state_guard = self.state.lock().await;

        let trades_state = TradingState::new(
            state_guard.last_tick_time,
            state_guard.balance.max(0) as u64,
            Price::bounded(state_guard.market_price),
            state_guard.last_trade_time,
            state_guard.running_map.clone().into_dyn(),
            state_guard.realized_pl,
            state_guard.closed_len,
            state_guard.closed_fees,
        );

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests;
