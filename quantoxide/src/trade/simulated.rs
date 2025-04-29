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

enum TradeSide {
    Long,
    Short,
}

struct SimulatedTradeRunning {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
}

struct SimulatedTradeClosed {
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
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
        let trades_state = TradesState::new(
            timestamp,
            0,
            0,
            0,
            None,
            None,
            0,
            0 - self.start_balance as i64,
        );

        Ok(trades_state)
    }
}
