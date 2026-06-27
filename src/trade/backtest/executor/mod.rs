use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use lnm_sdk::rest::v3::models::{
    ClientId, CrossLeverage, Leverage, OrderQuantity, Price, TradeSide, TradeSize,
};

use crate::db::models::{FundingSettlementRow, OhlcCandleRow};

use super::{
    super::{
        core::{
            ClosedTradeHistory, CrossOrderRequest, CrossPositionCore, IsolatedOrderRequest,
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
use models::{SimulatedCrossPosition, SimulatedTradeRunning};

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
    market_price: f64,
    balance: i64,
    last_trade_time: Option<DateTime<Utc>>,
    trigger: PriceTrigger,
    running_map: RunningTradesMap<SimulatedTradeRunning>,
    funding_fees: i64,
    realized_pl: i64,
    closed_history: Arc<ClosedTradeHistory>,
    closed_fees: u64,
    cross_position: SimulatedCrossPosition,
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
    ) -> Arc<Self> {
        let initial_state = SimulatedTradeExecutorState {
            time: start_candle.time,
            market_price: start_candle.open,
            balance: start_balance as i64,
            last_trade_time: None,
            trigger: PriceTrigger::new(),
            running_map: RunningTradesMap::new(),
            funding_fees: 0,
            realized_pl: 0,
            closed_history: Arc::new(ClosedTradeHistory::new()),
            closed_fees: 0,
            cross_position: SimulatedCrossPosition::initial(),
        };

        Arc::new(Self {
            config: config.into(),
            state: Arc::new(Mutex::new(initial_state)),
        })
    }

    /// Updates only the time, assuming no market price changes.
    pub async fn update_time(&self, time: DateTime<Utc>) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        if time < state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            });
        }

        state_guard.time = time;
        Ok(())
    }

    pub async fn candle_update(&self, candle: &OhlcCandleRow) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let time = candle.time + Duration::seconds(59);

        if time < state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            })?;
        }

        let mut new_last_trade_time = state_guard.last_trade_time;
        let mut new_cross_position = state_guard.cross_position;

        let cross_liquidated = state_guard.cross_position.liquidation_reached(candle);
        if cross_liquidated {
            new_cross_position = state_guard
                .cross_position
                .liquidate(self.config.fee_perc())?;

            new_last_trade_time = Some(time);
        }

        if !state_guard.trigger.was_reached(candle.low)
            && !state_guard.trigger.was_reached(candle.high)
        {
            state_guard.time = time;
            state_guard.market_price = candle.close;

            state_guard.last_trade_time = new_last_trade_time;
            state_guard.cross_position = new_cross_position;

            return Ok(());
        }

        // The market price reached some `stoploss` and/or `takeprofit`. Running
        // trades must be re-evaluated.

        let mut new_balance = state_guard.balance;
        let mut new_realized_pl = state_guard.realized_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut closed_trades: Vec<Arc<dyn TradeClosed>> = Vec::new();

        let mut new_trigger = PriceTrigger::new();
        let mut new_running_map = RunningTradesMap::new();

        for (trade, trade_tsl_opt) in state_guard.running_map.trades_desc_mut() {
            // Check if price reached stoploss or takeprofit

            let (trade_min_opt, trade_max_opt) = match trade.side() {
                TradeSide::Buy => (
                    Some(trade.stoploss().unwrap_or(trade.liquidation())),
                    trade.takeprofit(),
                ),
                TradeSide::Sell => (
                    trade.takeprofit(),
                    Some(trade.stoploss().unwrap_or(trade.liquidation())),
                ),
            };

            if let Some(trade_min) = trade_min_opt
                && candle.low <= trade_min.as_f64()
            {
                let closed_trade = trade.to_closed(self.config.fee_perc(), candle.time, trade_min);

                new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                new_realized_pl += closed_trade.pl();
                new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
                new_last_trade_time = Some(time);

                closed_trades.push(closed_trade);
                continue;
            }

            if let Some(trade_max) = trade_max_opt
                && candle.high >= trade_max.as_f64()
            {
                let closed_trade = trade.to_closed(self.config.fee_perc(), candle.time, trade_max);

                new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                new_realized_pl += closed_trade.pl();
                new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
                new_last_trade_time = Some(time);

                closed_trades.push(closed_trade);
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

                // Use candle.high for longs and candle.low for shorts
                let new_stoploss = match trade.side() {
                    TradeSide::Buy => {
                        let highest_price = Price::round(candle.high)
                            .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;
                        if highest_price >= next_stoploss_update_trigger {
                            let new_stoploss = highest_price
                                .apply_discount(trade_tsl.into())
                                .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;

                            if new_stoploss < market_price {
                                Some(new_stoploss)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    TradeSide::Sell => {
                        let lowest_price = Price::round(candle.low)
                            .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;
                        if lowest_price <= next_stoploss_update_trigger {
                            let new_stoploss = lowest_price
                                .apply_gain(trade_tsl.into())
                                .map_err(SimulatedTradeExecutorError::TickUpdatePriceValidation)?;

                            if new_stoploss > market_price {
                                Some(new_stoploss)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
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

        // Add closed trades to history after the loop to avoid borrow conflicts
        if !closed_trades.is_empty() {
            let closed_history = Arc::make_mut(&mut state_guard.closed_history);
            for closed_trade in closed_trades {
                closed_history
                    .add(closed_trade)
                    .map_err(SimulatedTradeExecutorError::ClosedHistoryUpdate)?;
            }
        }

        state_guard.time = time;
        state_guard.market_price = candle.close;

        state_guard.balance = new_balance;
        state_guard.last_trade_time = new_last_trade_time;

        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.realized_pl = new_realized_pl;
        state_guard.closed_fees = new_closed_fees;
        state_guard.cross_position = new_cross_position;

        Ok(())
    }

    pub async fn apply_funding_settlement(
        &self,
        settlement: &FundingSettlementRow,
    ) -> SimulatedTradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.running_map.is_empty() && state_guard.cross_position.quantity() == 0 {
            return Ok(());
        }

        let mut new_balance = state_guard.balance;
        let mut new_funding_fees = state_guard.funding_fees;
        let mut new_realized_pl = state_guard.realized_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_last_trade_time = state_guard.last_trade_time;
        let mut closed_trades: Vec<Arc<dyn TradeClosed>> = Vec::new();

        let mut new_trigger = PriceTrigger::new();
        let mut new_running_map = RunningTradesMap::new();

        for (trade, trade_tsl_opt) in state_guard.running_map.trades_desc() {
            let (updated_trade, funding_fee) = trade.apply_funding_settlement(settlement)?;

            new_funding_fees += funding_fee;

            if let Some(updated_trade) = updated_trade {
                if funding_fee < 0 {
                    new_balance -= funding_fee;
                }

                new_trigger
                    .update(
                        self.config.trailing_stoploss_step_size(),
                        updated_trade.as_ref(),
                        *trade_tsl_opt,
                    )
                    .map_err(SimulatedTradeExecutorError::PriceTriggerUpdate)?;
                new_running_map.add(updated_trade, *trade_tsl_opt);
            } else {
                // Edge case: The funding fee would make margin or leverage invalid, so the trade
                // can no longer be represented and should be closed at the current market price and
                // the fee deducted from the balance.
                // In practice this shouldn't happen. Trades that close to liquidation should be
                // closed by market movements first.
                let closing_price = Price::round(state_guard.market_price)
                    .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;
                let closed_trade =
                    trade.to_closed(self.config.fee_perc(), settlement.time, closing_price);

                new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl()
                    - funding_fee;

                new_realized_pl += closed_trade.pl();
                new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();
                new_last_trade_time = Some(settlement.time);

                closed_trades.push(closed_trade);
            }
        }

        if !closed_trades.is_empty() {
            let closed_history = Arc::make_mut(&mut state_guard.closed_history);
            for closed_trade in closed_trades {
                closed_history
                    .add(closed_trade)
                    .map_err(SimulatedTradeExecutorError::ClosedHistoryUpdate)?;
            }
        }

        let (new_cross_position, _cross_funding_fee, cross_forced_flattened) =
            state_guard.cross_position.apply_funding_settlement(
                state_guard.market_price,
                settlement,
                self.config.fee_perc(),
            )?;

        if cross_forced_flattened {
            new_last_trade_time = Some(settlement.time);
        }

        state_guard.balance = new_balance;
        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;
        state_guard.cross_position = new_cross_position;
        state_guard.funding_fees = new_funding_fees;
        state_guard.realized_pl = new_realized_pl;
        state_guard.closed_fees = new_closed_fees;
        state_guard.last_trade_time = new_last_trade_time;

        Ok(())
    }

    async fn close_running(&self, close: Close) -> SimulatedTradeExecutorResult<Vec<Uuid>> {
        let mut state_guard = self.state.lock().await;

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;

        let mut new_balance = state_guard.balance;
        let mut new_realized_pl = state_guard.realized_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut closed_ids = Vec::new();
        let mut closed_trades: Vec<Arc<dyn TradeClosed>> = Vec::new();

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
                let closed_trade = trade.to_closed(self.config.fee_perc(), time, market_price);

                new_balance += closed_trade.margin().as_i64() + closed_trade.maintenance_margin()
                    - closed_trade.closing_fee() as i64
                    + closed_trade.pl();

                new_realized_pl += closed_trade.pl();
                new_closed_fees += closed_trade.opening_fee() + closed_trade.closing_fee();

                closed_ids.push(trade.id());
                closed_trades.push(closed_trade);
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

        // Add closed trades to history after the loop to avoid borrow conflicts
        if !closed_trades.is_empty() {
            let closed_history = Arc::make_mut(&mut state_guard.closed_history);
            for closed_trade in closed_trades {
                closed_history
                    .add(closed_trade)
                    .map_err(SimulatedTradeExecutorError::ClosedHistoryUpdate)?;
            }
        }

        state_guard.balance = new_balance;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger = new_trigger;
        state_guard.running_map = new_running_map;

        state_guard.realized_pl = new_realized_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(closed_ids)
    }

    async fn execute_cross_market_order(
        &self,
        side: TradeSide,
        quantity: OrderQuantity,
    ) -> SimulatedTradeExecutorResult<Uuid> {
        let mut state_guard = self.state.lock().await;
        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;
        let new_cross_position = state_guard.cross_position.with_market_order(
            market_price,
            side,
            quantity.into(),
            self.config.fee_perc(),
        )?;
        let order_id = Uuid::new_v4();

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.cross_position = new_cross_position;

        Ok(order_id)
    }

    async fn create_running(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    ) -> SimulatedTradeExecutorResult<Uuid> {
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
            client_id,
        )?;

        let balance_delta = trade.margin().as_i64() + trade.maintenance_margin();
        if balance_delta > state_guard.balance {
            return Err(SimulatedTradeExecutorError::BalanceTooLow);
        }

        if state_guard.running_map.len() >= self.config.trade_max_running_qtd() {
            return Err(SimulatedTradeExecutorError::MaxRunningTradesReached {
                max_qtd: self.config.trade_max_running_qtd(),
            })?;
        }

        state_guard.balance -=
            trade.margin().as_i64() + trade.maintenance_margin() + trade.opening_fee() as i64;

        state_guard.last_trade_time = trade.filled_at();

        let trade_id = trade.id();

        state_guard
            .trigger
            .update(
                self.config.trailing_stoploss_step_size(),
                trade.as_ref(),
                trade_tsl,
            )
            .map_err(SimulatedTradeExecutorError::PriceTriggerUpdate)?;
        state_guard.running_map.add(trade, trade_tsl);

        Ok(trade_id)
    }
}

#[async_trait]
impl TradeExecutor for SimulatedTradeExecutor {
    async fn isolated_order(&self, request: IsolatedOrderRequest) -> TradeExecutorResult<Uuid> {
        let (side, size, leverage, stoploss, takeprofit, client_id) =
            request.into_open_trade_parts();

        Ok(self
            .create_running(side, size, leverage, stoploss, takeprofit, client_id)
            .await?)
    }

    async fn isolated_trade_add_margin(
        &self,
        trade_id: Uuid,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.balance < amount.get() as i64 {
            return Err(SimulatedTradeExecutorError::BalanceTooLow)?;
        }

        let Some((trade, _)) = state_guard.running_map.get_by_id_mut(trade_id) else {
            return Err(SimulatedTradeExecutorError::TradeNotRunning { trade_id })?;
        };

        let updated_trade = trade.with_added_margin(amount)?;

        *trade = updated_trade;
        state_guard.balance -= amount.get() as i64;

        Ok(())
    }

    async fn isolated_trade_cash_in(
        &self,
        trade_id: Uuid,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<()> {
        let mut state_guard = self.state.lock().await;

        let market_price = Price::bounded(state_guard.market_price);

        let Some((trade, _)) = state_guard.running_map.get_by_id_mut(trade_id) else {
            return Err(SimulatedTradeExecutorError::TradeNotRunning { trade_id })?;
        };

        let updated_trade = trade.with_cash_in(market_price, amount)?;

        let cashed_in_pl = trade.est_pl(updated_trade.price()).round() as i64;

        *trade = updated_trade;

        state_guard.balance += amount.get() as i64;
        state_guard.realized_pl += cashed_in_pl;

        Ok(())
    }

    async fn isolated_order_close(&self, trade_id: Uuid) -> TradeExecutorResult<()> {
        self.close_running(Close::Single(trade_id)).await?;
        Ok(())
    }

    async fn isolated_order_close_longs(&self) -> TradeExecutorResult<Vec<Uuid>> {
        Ok(self.close_running(TradeSide::Buy.into()).await?)
    }

    async fn isolated_order_close_shorts(&self) -> TradeExecutorResult<Vec<Uuid>> {
        Ok(self.close_running(TradeSide::Sell.into()).await?)
    }

    async fn isolated_order_close_all(&self) -> TradeExecutorResult<Vec<Uuid>> {
        Ok(self.close_running(Close::All).await?)
    }

    async fn cross_deposit(
        &self,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let mut state_guard = self.state.lock().await;
        let amount_i64 = i64::try_from(amount.get())
            .map_err(|_| SimulatedTradeExecutorError::CrossPositionOverflow)?;
        let market_price = Price::bounded(state_guard.market_price);

        if state_guard.balance < amount_i64 {
            return Err(SimulatedTradeExecutorError::BalanceTooLow)?;
        }

        let cross_position = state_guard.cross_position;
        let new_cross_margin = cross_position
            .margin()
            .checked_add(amount.get())
            .ok_or(SimulatedTradeExecutorError::CrossPositionOverflow)?;
        let new_cross_position = cross_position.with_margin(new_cross_margin)?;
        if !new_cross_position.is_coherent(market_price) {
            return Err(SimulatedTradeExecutorError::CrossPositionIncoherent)?;
        }

        state_guard.balance -= amount_i64;
        state_guard.cross_position = new_cross_position;

        Ok(Arc::new(state_guard.cross_position))
    }

    async fn cross_withdraw(
        &self,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let mut state_guard = self.state.lock().await;
        let amount_i64 = i64::try_from(amount.get())
            .map_err(|_| SimulatedTradeExecutorError::CrossPositionOverflow)?;
        let cross_position = state_guard.cross_position;
        let market_price = Price::bounded(state_guard.market_price);

        let free_margin = cross_position.est_free_margin(market_price);
        if amount.get() > free_margin
            || (cross_position.quantity() != 0 && amount.get() == free_margin)
        {
            return Err(SimulatedTradeExecutorError::CrossFreeMarginTooLow)?;
        }

        let balance = state_guard
            .balance
            .checked_add(amount_i64)
            .ok_or(SimulatedTradeExecutorError::CrossPositionOverflow)?;
        let new_cross_margin = cross_position.margin() - amount.get();
        let new_cross_position = cross_position.with_margin(new_cross_margin)?;
        if !new_cross_position.is_coherent(market_price) {
            return Err(SimulatedTradeExecutorError::CrossPositionIncoherent)?;
        }

        state_guard.balance = balance;
        state_guard.cross_position = new_cross_position;

        Ok(Arc::new(state_guard.cross_position))
    }

    async fn cross_set_leverage(
        &self,
        leverage: CrossLeverage,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let mut state_guard = self.state.lock().await;
        let cross_position = state_guard.cross_position;
        let market_price = Price::bounded(state_guard.market_price);

        let new_cross_position = cross_position.with_leverage(leverage)?;
        if !new_cross_position.is_coherent(market_price) {
            return Err(SimulatedTradeExecutorError::CrossPositionIncoherent)?;
        }

        state_guard.cross_position = new_cross_position;

        Ok(Arc::new(state_guard.cross_position))
    }

    async fn cross_order(&self, request: CrossOrderRequest) -> TradeExecutorResult<Uuid> {
        let (side, quantity, _client_id) = request.into_cross_order_parts();

        Ok(self.execute_cross_market_order(side, quantity).await?)
    }

    async fn cross_order_close_position(&self) -> TradeExecutorResult<Option<Uuid>> {
        let mut state_guard = self.state.lock().await;

        if state_guard.cross_position.quantity() == 0 {
            return Ok(None);
        }

        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::InvalidMarketPrice)?;
        let new_cross_position = state_guard
            .cross_position
            .close(market_price, self.config.fee_perc())?;
        let order_id = Uuid::new_v4();

        state_guard.cross_position = new_cross_position;
        state_guard.last_trade_time = Some(state_guard.time);

        Ok(Some(order_id))
    }

    async fn trading_state(&self) -> TradeExecutorResult<TradingState> {
        let state_guard = self.state.lock().await;

        let trades_state = TradingState::new(
            state_guard.time,
            state_guard.balance.max(0) as u64,
            Price::bounded(state_guard.market_price),
            state_guard.last_trade_time,
            state_guard.running_map.clone().into_dyn(),
            state_guard.funding_fees,
            state_guard.realized_pl,
            state_guard.closed_history.clone(),
            state_guard.closed_fees,
            Arc::new(state_guard.cross_position),
        );

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests;
