use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, SATS_PER_BTC, TradeSide,
};

use super::{TradesManager, TradesState, error::Result};

pub mod error;
mod models;

use error::{Result as SimulationResult, SimulationError};
use models::{RiskParams, SimulatedTradeClosed, SimulatedTradeRunning};

enum Close {
    Side(TradeSide),
    All,
}

impl From<TradeSide> for Close {
    fn from(value: TradeSide) -> Self {
        Self::Side(value)
    }
}

enum Trigger {
    NotSet,
    Set { min: Price, max: Price },
}

impl Trigger {
    fn new() -> Self {
        Self::NotSet
    }

    fn update(&mut self, trade: &SimulatedTradeRunning) {
        let mut new_min = trade.stoploss().min(trade.takeprofit());
        let mut new_max = trade.stoploss().max(trade.takeprofit());

        if let Trigger::Set { min, max } = *self {
            new_min = new_min.max(min);
            new_max = new_max.min(max);
        }

        *self = Trigger::Set {
            min: new_min,
            max: new_max,
        };
    }

    fn was_reached(&self, market_price: f64) -> bool {
        match self {
            Trigger::NotSet => false,
            Trigger::Set { min, max } => {
                market_price <= min.into_f64() || market_price >= max.into_f64()
            }
        }
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    market_price: f64,
    balance: i64,
    last_trade_time: Option<DateTime<Utc>>,
    trigger: Trigger,
    running: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
    closed_pl: i64,
    closed_fees: u64,
}

pub struct SimulatedTradesManager {
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<SimulatedTradesState>>,
}

impl SimulatedTradesManager {
    pub fn new(
        max_running_qtd: usize,
        fee_perc: BoundedPercentage,
        start_time: DateTime<Utc>,
        market_price: f64,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState {
            time: start_time,
            market_price,
            balance: start_balance as i64,
            last_trade_time: None,
            trigger: Trigger::new(),
            running: Vec::new(),
            closed: Vec::new(),
            closed_pl: 0,
            closed_fees: 0,
        };

        Self {
            max_running_qtd,
            fee_perc,
            start_time,
            start_balance,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    pub async fn tick_update(
        &self,
        time: DateTime<Utc>,
        market_price: f64,
    ) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if time <= state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            });
        }

        state_guard.time = time;
        state_guard.market_price = market_price;

        if !state_guard.trigger.was_reached(market_price) {
            return Ok(());
        }

        // The market price reached some `stoploss` and/or `takeprofit`. Running
        // trades must be re-evaluated.

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning, close_price: Price| {
            let closing_fee_reserved = trade.closing_fee_reserved() as i64;
            let trade = SimulatedTradeClosed::from_running(trade, time, close_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee() as i64;

            new_balance += trade.margin().into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee() + trade.closing_fee();
            new_closed_trades.push(trade);
        };

        let mut new_trigger = Trigger::new();
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let (min, max) = match trade.side() {
                TradeSide::Buy => (trade.stoploss(), trade.takeprofit()),
                TradeSide::Sell => (trade.takeprofit(), trade.stoploss()),
            };

            if market_price <= min.into_f64() {
                close_trade(trade, min);
            } else if market_price >= max.into_f64() {
                close_trade(trade, max);
            } else {
                new_trigger.update(&trade);
                remaining_running_trades.push(trade);
            }
        }

        state_guard.balance = new_balance;

        state_guard.trigger = new_trigger;
        state_guard.running = remaining_running_trades;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn close_running(&self, close: Close) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)?;

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning| {
            let closing_fee_reserved = trade.closing_fee_reserved() as i64;
            let trade =
                SimulatedTradeClosed::from_running(trade, time, market_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee() as i64;

            new_balance += trade.margin().into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee() + trade.closing_fee();
            new_closed_trades.push(trade);
        };

        let mut new_trigger = Trigger::new();
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            let should_be_closed = match &close {
                Close::Side(side) if *side == trade.side() => true,
                Close::All => true,
                _ => false,
            };

            if should_be_closed {
                close_trade(trade);
            } else {
                new_trigger.update(&trade);
                remaining_running_trades.push(trade);
            }
        }

        state_guard.balance = new_balance;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger = new_trigger;
        state_guard.running = remaining_running_trades;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn create_running(
        &self,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
        risk_params: RiskParams,
    ) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if state_guard.running.len() >= self.max_running_qtd {
            return Err(SimulationError::MaxRunningTradesReached {
                max_qtd: self.max_running_qtd,
            });
        }

        let market_price = Price::round(state_guard.market_price)?;

        let quantity = {
            let balance_usd = state_guard.balance as f64 * market_price.into_f64() / SATS_PER_BTC;
            let quantity_target = balance_usd * balance_perc.into_f64() / 100.;
            Quantity::try_from(quantity_target.floor())?
        };

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

        state_guard.balance -= trade.margin().into_i64() + trade.maintenance_margin() as i64;

        state_guard.last_trade_time = Some(state_guard.time);
        state_guard.trigger.update(&trade);
        state_guard.running.push(trade);

        Ok(())
    }
}

#[async_trait]
impl TradesManager for SimulatedTradesManager {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(balance_perc, leverage, risk_params)
            .await?;

        Ok(())
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(balance_perc, leverage, risk_params)
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

    async fn state(&self) -> Result<TradesState> {
        let state_guard = self.state.lock().await;

        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;
        let mut running_maintenance_margin: u64 = 0;

        // Use `Price::round_down` for long trades and `Price::round_up` for
        // short trades, in order to obtain more conservative prices. It is
        // expected that prices won't need to be rounded most of the time.

        for trade in state_guard.running.iter() {
            let market_price = match trade.side() {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin += trade.margin().into_u64();

                    Price::round_down(state_guard.market_price).map_err(SimulationError::from)?
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin += trade.margin().into_u64();

                    Price::round_up(state_guard.market_price).map_err(SimulationError::from)?
                }
            };

            running_pl += trade.pl(market_price);
            running_fees += trade.opening_fee();
            running_maintenance_margin += trade.maintenance_margin();
        }

        let trades_state = TradesState::new(
            self.start_time,
            self.start_balance,
            state_guard.time,
            state_guard.balance.max(0) as u64,
            state_guard.market_price,
            state_guard.last_trade_time,
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            running_fees,
            running_maintenance_margin,
            state_guard.closed.len(),
            state_guard.closed_pl,
            state_guard.closed_fees,
        );

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests;
