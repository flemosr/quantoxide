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
    pub fn pl(&self, current_price: Price) -> i64 {
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
    pub fn pl(&self) -> i64 {
        let price_diff = match self.side {
            TradeSide::Long => self.close_price.into_f64() - self.entry_price.into_f64(),
            TradeSide::Short => self.entry_price.into_f64() - self.close_price.into_f64(),
        };
        let pl = price_diff * self.margin.into_u64() as f64 * self.leverage.into_f64();
        pl as i64
    }
}

pub struct SimulatedTradesManager {
    db: Arc<DbContext>,
    max_qtd_trades_running: usize,
    start_time: DateTime<Utc>,
    start_balance: u64,
    balance: Arc<Mutex<u64>>,
    time: Arc<Mutex<DateTime<Utc>>>,
    running: Arc<Mutex<Vec<SimulatedTradeRunning>>>,
    closed: Arc<Mutex<Vec<SimulatedTradeClosed>>>,
}

impl SimulatedTradesManager {
    pub fn new(
        db: Arc<DbContext>,
        max_qtd_trades_running: usize,
        start_balance: u64,
        start_time: DateTime<Utc>,
    ) -> Self {
        Self {
            db,
            max_qtd_trades_running,
            start_time,
            start_balance,
            balance: Arc::new(Mutex::new(start_balance)),
            time: Arc::new(Mutex::new(start_time)),
            running: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(Mutex::new(Vec::new())),
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
                let mut timestamp_guard = self.time.lock().await;

                if timestamp <= *timestamp_guard {
                    return Err(TradeError::Generic(format!(
                        "received order with timestamp {timestamp} and current time is {}",
                        *timestamp_guard
                    )));
                }

                let mut running_guard = self.running.lock().await;

                if running_guard.len() >= self.max_qtd_trades_running {
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

                let mut balance_guard = self.balance.lock().await;

                let margin = {
                    let margin = *balance_guard as f64 * balance_perc.into_f64() / 100.;
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

                *timestamp_guard = timestamp;
                running_guard.push(trade);
                *balance_guard -= margin.into_u64();

                Ok(())
            }
            TradeOrder::OpenShort {
                timestamp,
                stoploss_perc,
                takeprofit_perc,
                balance_perc,
                leverage,
            } => {
                let mut timestamp_guard = self.time.lock().await;

                if timestamp <= *timestamp_guard {
                    return Err(TradeError::Generic(format!(
                        "received order with timestamp {timestamp} and current time is {}",
                        *timestamp_guard
                    )));
                }

                let mut running_guard = self.running.lock().await;

                if running_guard.len() >= self.max_qtd_trades_running {
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

                let mut balance_guard = self.balance.lock().await;

                let margin = {
                    let margin = *balance_guard as f64 * balance_perc.into_f64() / 100.;
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

                *timestamp_guard = timestamp;
                running_guard.push(trade);
                *balance_guard -= margin.into_u64();

                Ok(())
            }
            TradeOrder::CloseLongs { timestamp } => Ok(()),
            TradeOrder::CloseShorts { timestamp } => Ok(()),
            TradeOrder::CloseAll { timestamp } => Ok(()),
        }
    }

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState> {
        // Check if timestamp is valid
        let current_time_guard = *self.time.lock().await;
        if timestamp <= current_time_guard {
            return Err(TradeError::Generic(format!(
                "received state request with timestamp {timestamp} but current time is {current_time_guard}"
            )));
        }

        let mut running_guard = self.running.lock().await;
        let mut closed_guard = self.closed.lock().await;

        let mut remaining_running_trades = Vec::new();

        for trade in running_guard.drain(..) {
            // Check if price reached stoploss or takeprofit between
            // `current_time_guard` and `timestamp`.

            let (min, max) = match trade.side {
                TradeSide::Long => (trade.stoploss.into_f64(), trade.takeprofit.into_f64()),
                TradeSide::Short => (trade.takeprofit.into_f64(), trade.stoploss.into_f64()),
            };

            let boundary_entry_opt = self
                .db
                .price_history
                .get_first_entry_reaching_bounds(current_time_guard, timestamp, min, max)
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

                closed_guard.push(SimulatedTradeClosed {
                    side: trade.side,
                    entry_time: trade.entry_time,
                    entry_price: trade.entry_price,
                    stoploss: trade.stoploss,
                    takeprofit: trade.takeprofit,
                    margin: trade.margin,
                    leverage: trade.leverage,
                    close_time: price_entry.time,
                    close_price,
                });
            } else {
                // Trade still running

                remaining_running_trades.push(trade);
            }
        }

        *running_guard = remaining_running_trades;

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

        for trade in running_guard.iter() {
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

        let closed_pl = closed_guard.iter().map(|trade| trade.pl()).sum::<i64>();

        let calc_balance =
            self.start_balance as i64 - total_margin_long as i64 - total_margin_short as i64
                + closed_pl;

        let mut balance_guard = self.balance.lock().await;
        *balance_guard = if calc_balance < 0 {
            0
        } else {
            calc_balance as u64
        };

        let mut time_guard = self.time.lock().await;
        *time_guard = timestamp;

        let locked_margin_long = if total_margin_long > 0 {
            let margin = Margin::try_from(total_margin_long)
                .map_err(|e| TradeError::Generic(e.to_string()))?;
            Some(margin)
        } else {
            None
        };

        let locked_margin_short = if total_margin_short > 0 {
            let margin = Margin::try_from(total_margin_short)
                .map_err(|e| TradeError::Generic(e.to_string()))?;
            Some(margin)
        } else {
            None
        };

        let trades_state = TradesState::new(
            timestamp,
            qtd_trades_running_long,
            qtd_trades_running_short,
            closed_guard.len(),
            locked_margin_long,
            locked_margin_short,
            *balance_guard,
            running_pl,
            closed_pl,
        );

        Ok(trades_state)
    }
}
