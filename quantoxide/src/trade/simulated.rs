use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;

use lnm_sdk::api::rest::models::{Leverage, Margin, Price, Quantity};

use crate::db::DbContext;

use super::{
    TradeOrder, TradesManager, TradesState,
    error::{Result, TradeError},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TradeSide {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulatedTradeRunning {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
}

impl SimulatedTradeRunning {
    fn pl(&self, current_price: Price) -> i64 {
        let price_diff = match self.side {
            TradeSide::Long => current_price.into_f64() - self.entry_price.into_f64(),
            TradeSide::Short => self.entry_price.into_f64() - current_price.into_f64(),
        };
        let pl = price_diff * self.margin.into_u64() as f64 * self.leverage.into_f64();
        pl as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulatedTradeClosed {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
}

impl SimulatedTradeClosed {
    fn from_running(
        running: SimulatedTradeRunning,
        close_time: DateTime<Utc>,
        close_price: Price,
    ) -> Self {
        SimulatedTradeClosed {
            side: running.side,
            entry_time: running.entry_time,
            entry_price: running.entry_price,
            stoploss: running.stoploss,
            takeprofit: running.takeprofit,
            margin: running.margin,
            leverage: running.leverage,
            close_time,
            close_price,
        }
    }

    fn pl(&self) -> i64 {
        let price_diff = match self.side {
            TradeSide::Long => self.close_price.into_f64() - self.entry_price.into_f64(),
            TradeSide::Short => self.entry_price.into_f64() - self.close_price.into_f64(),
        };
        let pl = price_diff * self.margin.into_u64() as f64 * self.leverage.into_f64();
        pl as i64
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    balance: u64,
    running: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
    // locked_margin_long: Option<Margin>,
    // locked_margin_short: Option<Margin>,
    // running_pl: i64,
    // closed_pl: i64,
}

impl SimulatedTradesState {
    fn new(start_time: DateTime<Utc>, start_balance: u64) -> Self {
        Self {
            time: start_time,
            balance: start_balance,
            running: Vec::new(),
            closed: Vec::new(),
            // locked_margin_long: None,
            // locked_margin_short: None,
            // running_pl: 0,
            // closed_pl: 0,
        }
    }

    async fn update(&mut self, db: &DbContext, new_time: DateTime<Utc>) -> Result<()> {
        if new_time <= self.time {
            return Err(TradeError::Generic(format!(
                "tried to update state with new_time {new_time} but current time is {}",
                self.time
            )));
        }

        let previous_time = self.time;
        let mut remaining_running_trades = Vec::new();
        let mut new_closed_trades = Vec::new();

        for trade in self.running.drain(..) {
            // Check if price reached stoploss or takeprofit between
            // `current_time_guard` and `timestamp`.

            let (min, max) = match trade.side {
                TradeSide::Long => (trade.stoploss.into_f64(), trade.takeprofit.into_f64()),
                TradeSide::Short => (trade.takeprofit.into_f64(), trade.stoploss.into_f64()),
            };

            let boundary_entry_opt = db
                .price_history
                .get_first_entry_reaching_bounds(previous_time, new_time, min, max)
                .await
                .map_err(|e| TradeError::Generic(e.to_string()))?;

            if let Some(price_entry) = boundary_entry_opt {
                // Trade closed

                let close_price = match trade.side {
                    TradeSide::Long if price_entry.value <= min => trade.stoploss,
                    TradeSide::Long if price_entry.value >= max => trade.takeprofit,
                    TradeSide::Short if price_entry.value <= min => trade.takeprofit,
                    TradeSide::Short if price_entry.value >= max => trade.stoploss,
                    _ => return Err(TradeError::Generic("invalid".to_string())),
                };

                let closed_trade =
                    SimulatedTradeClosed::from_running(trade, price_entry.time, close_price);

                let balance_diff = closed_trade.margin.into_u64() as i64 + closed_trade.pl();

                self.balance = (self.balance as i64 + balance_diff).max(0) as u64;
                new_closed_trades.push(closed_trade);
            } else {
                // Trade still running

                remaining_running_trades.push(trade);
            }
        }

        self.running = remaining_running_trades;
        self.closed.append(&mut new_closed_trades);
        self.time = new_time;

        Ok(())
    }
}

pub struct SimulatedTradesManager {
    db: Arc<DbContext>,
    max_qtd_trades_running: usize,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<SimulatedTradesState>>,
}

impl SimulatedTradesManager {
    pub fn new(
        db: Arc<DbContext>,
        max_qtd_trades_running: usize,
        start_time: DateTime<Utc>,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState::new(start_time, start_balance);
        Self {
            db,
            max_qtd_trades_running,
            start_time,
            start_balance,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }
}

#[async_trait]
impl TradesManager for SimulatedTradesManager {
    async fn order(&self, order: TradeOrder) -> Result<()> {
        match order {
            TradeOrder::OpenLong {
                timestamp,
                stoploss_perc,
                takeprofit_perc,
                balance_perc,
                leverage,
            } => {
                let mut state_guard = self.state.lock().await;

                if timestamp <= state_guard.time {
                    return Err(TradeError::Generic(format!(
                        "received order with timestamp {timestamp} and current time is {}",
                        state_guard.time
                    )));
                }

                if state_guard.running.len() >= self.max_qtd_trades_running {
                    return Err(TradeError::Generic(format!(
                        "received order but max qtd of running trades ({}) was reached",
                        self.max_qtd_trades_running
                    )));
                }

                let market_price = {
                    let price_entry = self
                        .db
                        .price_history
                        .get_latest_entry_at_or_before(timestamp)
                        .await
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .ok_or(TradeError::Generic(format!(
                            "no price history entry was found with time at or before {}",
                            timestamp
                        )))?;
                    Price::try_from(price_entry.value)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                };

                let margin = {
                    let margin = state_guard.balance as f64 * balance_perc.into_f64() / 100.;
                    let margin = Margin::try_from(margin.floor())
                        .map_err(|e| TradeError::Generic(e.to_string()))?;
                    let _ = Quantity::try_calculate(margin, market_price, leverage)
                        .map_err(|e| TradeError::Generic(e.to_string()))?;
                    margin
                };

                let stoploss = market_price
                    .apply_discount(stoploss_perc)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                let takeprofit = market_price
                    .apply_gain(takeprofit_perc)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                let trade = SimulatedTradeRunning {
                    side: TradeSide::Long,
                    entry_time: timestamp,
                    entry_price: market_price,
                    stoploss,
                    takeprofit,
                    margin,
                    leverage,
                };

                state_guard.time = timestamp;
                state_guard.running.push(trade);
                state_guard.balance -= margin.into_u64();

                Ok(())
            }
            TradeOrder::OpenShort {
                timestamp,
                stoploss_perc,
                takeprofit_perc,
                balance_perc,
                leverage,
            } => {
                let mut state_guard = self.state.lock().await;

                if timestamp <= state_guard.time {
                    return Err(TradeError::Generic(format!(
                        "received order with timestamp {timestamp} and current time is {}",
                        state_guard.time
                    )));
                }

                if state_guard.running.len() >= self.max_qtd_trades_running {
                    return Err(TradeError::Generic(format!(
                        "received order but max qtd of running trades ({}) was reached",
                        self.max_qtd_trades_running
                    )));
                }

                let market_price = {
                    let price_entry = self
                        .db
                        .price_history
                        .get_latest_entry_at_or_before(timestamp)
                        .await
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .ok_or(TradeError::Generic(format!(
                            "no price history entry was found with time at or before {}",
                            timestamp
                        )))?;
                    Price::try_from(price_entry.value)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                };

                let margin = {
                    let margin = state_guard.balance as f64 * balance_perc.into_f64() / 100.;
                    let margin = Margin::try_from(margin.floor())
                        .map_err(|e| TradeError::Generic(e.to_string()))?;
                    let _ = Quantity::try_calculate(margin, market_price, leverage)
                        .map_err(|e| TradeError::Generic(e.to_string()))?;
                    margin
                };

                let stoploss = market_price
                    .apply_gain(stoploss_perc.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                let takeprofit = market_price
                    .apply_discount(takeprofit_perc)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                let trade = SimulatedTradeRunning {
                    side: TradeSide::Short,
                    entry_time: timestamp,
                    entry_price: market_price,
                    stoploss,
                    takeprofit,
                    margin,
                    leverage,
                };

                state_guard.time = timestamp;
                state_guard.running.push(trade);
                state_guard.balance -= margin.into_u64();

                Ok(())
            }
            TradeOrder::CloseLongs { timestamp } => {
                // Update the state in order to determine which trades are still
                // running.

                let mut state_guard = self.state.lock().await;

                state_guard.update(self.db.as_ref(), timestamp).await?;

                // Get the current market price

                let market_price = {
                    let price_entry = self
                        .db
                        .price_history
                        .get_latest_entry_at_or_before(timestamp)
                        .await
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .ok_or(TradeError::Generic(format!(
                            "no price history entry was found with time at or before {}",
                            timestamp
                        )))?;
                    Price::try_from(price_entry.value)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                };

                // Update the state, moving all running long trades to 'closed',
                // assume they were closed at the current market price.

                let mut remaining_running_trades = Vec::new();
                let mut new_closed_trades = Vec::new();
                let mut new_balance = state_guard.balance as i64;

                for trade in state_guard.running.drain(..) {
                    if trade.side == TradeSide::Long {
                        let closed_trade =
                            SimulatedTradeClosed::from_running(trade, timestamp, market_price);

                        let balance_diff =
                            closed_trade.margin.into_u64() as i64 + closed_trade.pl();

                        new_balance += balance_diff;
                        new_closed_trades.push(closed_trade);
                    } else {
                        remaining_running_trades.push(trade);
                    }
                }

                state_guard.running = remaining_running_trades;
                state_guard.closed.append(&mut new_closed_trades);
                state_guard.balance = new_balance.max(0) as u64;

                Ok(())
            }
            TradeOrder::CloseShorts { timestamp } => Ok(()),
            TradeOrder::CloseAll { timestamp } => Ok(()),
        }
    }

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState> {
        let mut state_guard = self.state.lock().await;

        state_guard.update(self.db.as_ref(), timestamp).await?;

        let market_price = {
            let price_entry = self
                .db
                .price_history
                .get_latest_entry_at_or_before(timestamp)
                .await
                .map_err(|e| TradeError::Generic(e.to_string()))?
                .ok_or(TradeError::Generic(format!(
                    "no price history entry was found with time at or before {}",
                    timestamp
                )))?;
            Price::try_from(price_entry.value).map_err(|e| TradeError::Generic(e.to_string()))?
        };

        let mut total_margin_long: u64 = 0;
        let mut qtd_trades_running_long: usize = 0;
        let mut total_margin_short: u64 = 0;
        let mut qtd_trades_running_short: usize = 0;
        let mut running_pl: i64 = 0;

        for trade in state_guard.running.iter() {
            match trade.side {
                TradeSide::Long => {
                    total_margin_long += trade.margin.into_u64();
                    qtd_trades_running_long += 1;
                }
                TradeSide::Short => {
                    total_margin_short += trade.margin.into_u64();
                    qtd_trades_running_short += 1;
                }
            }
            running_pl += trade.pl(market_price);
        }

        let closed_pl = state_guard
            .closed
            .iter()
            .map(|trade| trade.pl())
            .sum::<i64>();

        let locked_margin_long = (total_margin_long > 0)
            .then(|| Margin::try_from(total_margin_long))
            .transpose()
            .map_err(|e| TradeError::Generic(e.to_string()))?;

        let locked_margin_short = (total_margin_short > 0)
            .then(|| Margin::try_from(total_margin_short))
            .transpose()
            .map_err(|e| TradeError::Generic(e.to_string()))?;

        let trades_state = TradesState::new(
            timestamp,
            qtd_trades_running_long,
            qtd_trades_running_short,
            state_guard.closed.len(),
            locked_margin_long,
            locked_margin_short,
            state_guard.balance,
            running_pl,
            closed_pl,
        );

        Ok(trades_state)
    }
}
