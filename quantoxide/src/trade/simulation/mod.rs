use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Margin, Price, Quantity, TradeSide,
};

use crate::db::models::PriceHistoryEntry;

use super::{TradesManager, TradesState, error::Result};

pub mod error;
mod models;

use error::{Result as SimulationResult, SimulationError};
use models::{RiskParams, SimulatedTradeClosed, SimulatedTradeRunning};

const SATS_PER_BTC: f64 = 100_000_000.;

enum Close {
    Side(TradeSide),
    All,
}

impl From<TradeSide> for Close {
    fn from(value: TradeSide) -> Self {
        Self::Side(value)
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    market_price: Price,
    balance: i64,
    running: Vec<SimulatedTradeRunning>,
    running_long_qtd: usize,
    running_long_margin: Option<Margin>,
    running_short_qtd: usize,
    running_short_margin: Option<Margin>,
    running_pl: i64,
    running_fees_est: u64,
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
        market_price: Price,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState {
            time: start_time,
            market_price,
            balance: start_balance as i64,
            running: Vec::new(),
            running_long_qtd: 0,
            running_long_margin: None,
            running_short_qtd: 0,
            running_short_margin: None,
            running_pl: 0,
            running_fees_est: 0,
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

    pub async fn tick_update(&self, new_entry: &PriceHistoryEntry) -> SimulationResult<()> {
        let new_time = new_entry.time;
        let market_price = Price::try_from(new_entry.value)?;

        let mut state_guard = self.state.lock().await;

        if new_time <= state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time,
                current_time: state_guard.time,
            });
        }

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning, close_price: Price| {
            let closing_fee_reserved = trade.closing_fee_reserved as i64;
            let trade =
                SimulatedTradeClosed::from_running(trade, new_time, close_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee as i64;

            new_balance += trade.margin.into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee + trade.closing_fee;
            new_closed_trades.push(trade);
        };

        let mut new_running_long_qtd: usize = 0;
        let mut new_running_long_margin: u64 = 0;
        let mut new_running_short_qtd: usize = 0;
        let mut new_running_short_margin: u64 = 0;
        let mut new_running_pl: i64 = 0;
        let mut new_running_fees_est: u64 = 0;
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let (min, max) = match trade.side {
                TradeSide::Buy => (trade.stoploss, trade.takeprofit),
                TradeSide::Sell => (trade.takeprofit, trade.stoploss),
            };

            if market_price <= min {
                close_trade(trade, min);
            } else if market_price >= max {
                close_trade(trade, max);
            } else {
                // Trade still running

                match trade.side {
                    TradeSide::Buy => {
                        new_running_long_qtd += 1;
                        new_running_long_margin += trade.margin.into_u64();
                    }
                    TradeSide::Sell => {
                        new_running_short_qtd += 1;
                        new_running_short_margin += trade.margin.into_u64();
                    }
                }
                new_running_pl += trade.pl(market_price);
                new_running_fees_est += trade.opening_fee + trade.closing_fee_reserved;
                remaining_running_trades.push(trade);
            }
        }

        state_guard.time = new_time;
        state_guard.market_price = market_price;
        state_guard.balance = new_balance;

        state_guard.running = remaining_running_trades;
        state_guard.running_long_qtd = new_running_long_qtd;
        state_guard.running_long_margin = (new_running_long_margin > 0)
            .then(|| Margin::try_from(new_running_long_margin))
            .transpose()?;
        state_guard.running_short_qtd = new_running_short_qtd;
        state_guard.running_short_margin = (new_running_short_margin > 0)
            .then(|| Margin::try_from(new_running_short_margin))
            .transpose()?;
        state_guard.running_pl = new_running_pl;
        state_guard.running_fees_est = new_running_fees_est;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn close_running(&self, timestamp: DateTime<Utc>, close: Close) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if timestamp <= state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: timestamp,
                current_time: state_guard.time,
            });
        }

        let time = state_guard.time;
        let market_price = state_guard.market_price;

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning| {
            let closing_fee_reserved = trade.closing_fee_reserved as i64;
            let trade =
                SimulatedTradeClosed::from_running(trade, time, market_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee as i64;

            new_balance += trade.margin.into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee + trade.closing_fee;
            new_closed_trades.push(trade);
        };

        let mut new_running_long_qtd: usize = 0;
        let mut new_running_long_margin: u64 = 0;
        let mut new_running_short_qtd: usize = 0;
        let mut new_running_short_margin: u64 = 0;
        let mut new_running_pl: i64 = 0;
        let mut new_running_fees_est: u64 = 0;
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let should_be_closed = match &close {
                Close::Side(side) if *side == trade.side => true,
                Close::All => true,
                _ => false,
            };

            if should_be_closed {
                close_trade(trade);
            } else {
                match trade.side {
                    TradeSide::Buy => {
                        new_running_long_qtd += 1;
                        new_running_long_margin += trade.margin.into_u64();
                    }
                    TradeSide::Sell => {
                        new_running_short_qtd += 1;
                        new_running_short_margin += trade.margin.into_u64();
                    }
                }
                new_running_pl += trade.pl(market_price);
                new_running_fees_est += trade.opening_fee + trade.closing_fee_reserved;
                remaining_running_trades.push(trade);
            }
        }

        state_guard.time = timestamp;
        state_guard.balance = new_balance;

        state_guard.running = remaining_running_trades;
        state_guard.running_long_qtd = new_running_long_qtd;
        state_guard.running_long_margin = (new_running_long_margin > 0)
            .then(|| Margin::try_from(new_running_long_margin))
            .transpose()?;
        state_guard.running_short_qtd = new_running_short_qtd;
        state_guard.running_short_margin = (new_running_short_margin > 0)
            .then(|| Margin::try_from(new_running_short_margin))
            .transpose()?;
        state_guard.running_pl = new_running_pl;
        state_guard.running_fees_est = new_running_fees_est;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn create_running(
        &self,
        timestamp: DateTime<Utc>,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
        risk_params: RiskParams,
    ) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if timestamp <= state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: timestamp,
                current_time: state_guard.time,
            });
        }

        if state_guard.running.len() >= self.max_running_qtd {
            return Err(SimulationError::MaxRunningTradesReached {
                max_qtd: self.max_running_qtd,
            });
        }

        let quantity = {
            let balance_usd =
                state_guard.balance as f64 * state_guard.market_price.into_f64() / SATS_PER_BTC;
            let quantity = balance_usd * balance_perc.into_f64() / 100.;
            Quantity::try_from(quantity.floor())?
        };

        let (side, stoploss, takeprofit) =
            risk_params.into_trade_params(state_guard.market_price)?;

        let trade = SimulatedTradeRunning::new(
            side,
            timestamp,
            state_guard.market_price,
            stoploss,
            takeprofit,
            quantity,
            leverage,
            self.fee_perc,
        )?;

        state_guard.time = timestamp;
        state_guard.balance -=
            trade.margin.into_i64() - trade.opening_fee as i64 - trade.closing_fee_reserved as i64;
        state_guard.running.push(trade);

        Ok(())
    }
}

#[async_trait]
impl TradesManager for SimulatedTradesManager {
    async fn open_long(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(timestamp, balance_perc, leverage, risk_params)
            .await?;

        Ok(())
    }

    async fn open_short(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(timestamp, balance_perc, leverage, risk_params)
            .await?;

        Ok(())
    }

    async fn close_longs(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.close_running(timestamp, TradeSide::Buy.into()).await?;

        Ok(())
    }

    async fn close_shorts(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self
            .close_running(timestamp, TradeSide::Sell.into())
            .await?;

        Ok(())
    }

    async fn close_all(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.close_running(timestamp, Close::All).await?;

        Ok(())
    }

    async fn state(&self) -> Result<TradesState> {
        let state_guard = self.state.lock().await;

        let trades_state = TradesState {
            start_time: self.start_time,
            start_balance: self.start_balance,
            current_time: state_guard.time,
            current_balance: state_guard.balance.max(0) as u64,
            running_long_qtd: state_guard.running_long_qtd,
            running_long_margin: state_guard.running_long_margin,
            running_short_qtd: state_guard.running_short_qtd,
            running_short_margin: state_guard.running_short_margin,
            running_pl: state_guard.running_pl,
            running_fees_est: state_guard.running_fees_est,
            closed_qtd: state_guard.closed.len(),
            closed_pl: state_guard.closed_pl,
            closed_fees: state_guard.closed_fees,
        };

        Ok(trades_state)
    }
}
