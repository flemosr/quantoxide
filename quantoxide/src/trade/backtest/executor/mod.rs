use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, Trade, TradeSide,
    error::QuantityValidationError,
};

use super::super::{
    core::{
        PriceTrigger, RiskParams, StoplossMode, TradeExecutor, TradeExt, TradeTrailingStoploss,
        TradingState,
    },
    error::{Result, TradeError},
};

pub mod error;
mod models;

use error::SimulatedTradeExecutorError;
use models::{SimulatedTradeClosed, SimulatedTradeRunning};

enum Close {
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
    running: Vec<(Arc<SimulatedTradeRunning>, Option<TradeTrailingStoploss>)>,
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
            market_price,
            balance: start_balance as i64,
            last_trade_time: None,
            trigger: PriceTrigger::new(),
            running: Vec::new(),
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

    pub async fn tick_update(&self, time: DateTime<Utc>, market_price: f64) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        if time <= state_guard.time {
            return Err(SimulatedTradeExecutorError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            })?;
        }

        state_guard.time = time;
        state_guard.market_price = market_price;

        if !state_guard.trigger.was_reached(market_price) {
            return Ok(());
        }

        // The market price reached some `stoploss` and/or `takeprofit`. Running
        // trades must be re-evaluated.

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut close_trade = |trade: Arc<SimulatedTradeRunning>, close_price: Price| {
            let trade = SimulatedTradeClosed::from_running(
                trade.as_ref(),
                time,
                close_price,
                self.fee_perc,
            );

            new_balance += trade.margin().into_i64() + trade.maintenance_margin()
                - trade.closing_fee() as i64
                + trade.pl();

            new_closed_len += 1;
            new_closed_pl += trade.pl();
            new_closed_fees += trade.opening_fee() + trade.closing_fee();
        };

        let mut new_trigger = PriceTrigger::new();
        let mut remaining_running_trades = Vec::new();

        for (mut trade, trade_tsl_opt) in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let (trade_min_opt, trade_max_opt) = match trade.side() {
                TradeSide::Buy => (trade.stoploss(), trade.takeprofit()),
                TradeSide::Sell => (trade.takeprofit(), trade.stoploss()),
            };

            if let Some(trade_min) = trade_min_opt {
                if market_price <= trade_min.into_f64() {
                    close_trade(trade, trade_min);
                    continue;
                }
            }

            if let Some(trade_max) = trade_max_opt {
                if market_price >= trade_max.into_f64() {
                    close_trade(trade, trade_max);
                    continue;
                }
            }

            if let Some(trade_tsl) = trade_tsl_opt {
                let next_stoploss_update_trigger =
                    trade.next_stoploss_update_trigger(self.tsl_step_size, trade_tsl)?;

                let market_price = Price::round(market_price)
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
                    trade =
                        SimulatedTradeRunning::from_trade_with_new_stoploss(trade, new_stoploss)?;
                }
            }

            new_trigger.update(self.tsl_step_size, trade.as_ref(), trade_tsl_opt)?;
            remaining_running_trades.push((trade, trade_tsl_opt));
        }

        state_guard.balance = new_balance;

        state_guard.trigger = new_trigger;
        state_guard.running = remaining_running_trades;

        state_guard.closed_len = new_closed_len;
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn close_running(&self, close: Close) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::PriceValidation)?;

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_len = state_guard.closed_len;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;

        let mut close_trade = |trade: Arc<SimulatedTradeRunning>| {
            let trade = SimulatedTradeClosed::from_running(
                trade.as_ref(),
                time,
                market_price,
                self.fee_perc,
            );

            new_balance += trade.margin().into_i64() + trade.maintenance_margin()
                - trade.closing_fee() as i64
                + trade.pl();

            new_closed_len += 1;
            new_closed_pl += trade.pl();
            new_closed_fees += trade.opening_fee() + trade.closing_fee();
        };

        let mut new_trigger = PriceTrigger::new();
        let mut remaining_running_trades = Vec::new();

        for (trade, trade_tsl) in state_guard.running.drain(..) {
            let should_be_closed = match &close {
                Close::Side(side) if *side == trade.side() => true,
                Close::All => true,
                _ => false,
            };

            if should_be_closed {
                close_trade(trade);
            } else {
                new_trigger.update(self.tsl_step_size, trade.as_ref(), trade_tsl)?;
                remaining_running_trades.push((trade, trade_tsl));
            }
        }

        state_guard.balance = new_balance;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger = new_trigger;
        state_guard.running = remaining_running_trades;

        state_guard.closed_len = new_closed_len;
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn create_running(
        &self,
        risk_params: RiskParams,
        trade_tsl: Option<TradeTrailingStoploss>,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.running.len() >= self.max_running_qtd {
            return Err(SimulatedTradeExecutorError::MaxRunningTradesReached {
                max_qtd: self.max_running_qtd,
            })?;
        }

        let market_price = Price::round(state_guard.market_price)
            .map_err(SimulatedTradeExecutorError::PriceValidation)?;

        let quantity = Quantity::try_from_balance_perc(
            state_guard.balance.max(0) as u64,
            market_price,
            balance_perc,
        )
        .map_err(|e| match e {
            QuantityValidationError::TooLow => TradeError::BalanceTooLow,
            QuantityValidationError::TooHigh => TradeError::BalanceTooHigh,
        })?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let trade = SimulatedTradeRunning::new(
            side,
            state_guard.time,
            market_price,
            stoploss,
            takeprofit,
            quantity,
            leverage,
            self.fee_perc,
        )?;

        state_guard.balance -= trade.margin().into_i64()
            + trade.maintenance_margin() as i64
            + trade.opening_fee() as i64;

        state_guard.last_trade_time = Some(state_guard.time);

        state_guard
            .trigger
            .update(self.tsl_step_size, trade.as_ref(), trade_tsl)?;
        state_guard.running.push((trade, trade_tsl));

        Ok(())
    }
}

#[async_trait]
impl TradeExecutor for SimulatedTradeExecutor {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let trade_tsl = stoploss_mode.validate_trade_tsl(self.tsl_step_size, stoploss_perc)?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(risk_params, trade_tsl, balance_perc, leverage)
            .await?;

        Ok(())
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let trade_tsl = stoploss_mode.validate_trade_tsl(self.tsl_step_size, stoploss_perc)?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(risk_params, trade_tsl, balance_perc, leverage)
            .await?;

        Ok(())
    }

    async fn close_longs(&self) -> Result<()> {
        let _ = self.close_running(TradeSide::Buy.into()).await?;

        Ok(())
    }

    async fn close_shorts(&self) -> Result<()> {
        let _ = self.close_running(TradeSide::Sell.into()).await?;

        Ok(())
    }

    async fn close_all(&self) -> Result<()> {
        let _ = self.close_running(Close::All).await?;

        Ok(())
    }

    async fn trading_state(&self) -> Result<TradingState> {
        let state_guard = self.state.lock().await;

        let mut running: Vec<Arc<dyn Trade>> = Vec::new();
        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_long_quantity: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_short_quantity: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;

        // Use `Price::round_down` for long trades and `Price::round_up` for
        // short trades, in order to obtain more conservative prices. It is
        // expected that prices won't need to be rounded most of the time.

        for (trade, _) in &state_guard.running {
            let market_price = match trade.side() {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_long_quantity += trade.quantity().into_u64();

                    Price::round_down(state_guard.market_price)
                        .map_err(SimulatedTradeExecutorError::from)?
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_short_quantity += trade.quantity().into_u64();

                    Price::round_up(state_guard.market_price)
                        .map_err(SimulatedTradeExecutorError::from)?
                }
            };

            running_pl += trade.pl(market_price);
            running_fees += trade.opening_fee();
            running.push(trade.clone());
        }

        let trades_state = TradingState::new(
            state_guard.time,
            state_guard.balance.max(0) as u64,
            state_guard.market_price,
            state_guard.last_trade_time,
            // FIXME: Temporary workaround to avoid sending big `Vec`s over
            // channels.
            Vec::new(),
            // running,
            running_long_qtd,
            running_long_margin,
            running_long_quantity,
            running_short_qtd,
            running_short_margin,
            running_short_quantity,
            running_pl,
            running_fees,
            state_guard.closed_len,
            state_guard.closed_pl,
            state_guard.closed_fees,
        )
        .expect("`SimulatedTradeExecutor` can't contain inconsistent trades");

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests;
