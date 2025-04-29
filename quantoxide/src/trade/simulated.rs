use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

use lnm_sdk::api::rest::models::{Leverage, Price, Quantity};

use crate::db::DbContext;

use super::{
    TradeOrder, TradesManager, TradesState,
    error::{Result, TradeError},
};

struct SimulatedTradeRunning {
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    quantity: Quantity,
    leverage: Leverage,
}

struct SimulatedTradeClosed {
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    quantity: Quantity,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
}

pub struct SimulatedTradesManager {
    db: Arc<DbContext>,
    max_qtd_trades_running: usize,
    start_balance: u64,
    balance: u64,
    time: DateTime<Utc>,
    long: Vec<SimulatedTradeRunning>,
    short: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
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
            start_balance,
            balance: start_balance,
            time: start_time,
            long: Vec::new(),
            short: Vec::new(),
            closed: Vec::new(),
        }
    }

    fn max_running_trades_reached(&self) -> bool {
        (self.long.len() + self.short.len()) >= self.max_qtd_trades_running
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
                // Validate `timestamp`, must be gt than `self.timestamp`
                if timestamp <= self.time {
                    return Err(TradeError::Generic(format!(
                        "received order with timestamp {timestamp} and current time is {}",
                        self.time
                    )));
                }

                // Check `max_qtd_trades_running`

                if self.max_running_trades_reached() {
                    return Err(TradeError::Generic(format!(
                        "received order but max qtd of running trades ({}) was reached",
                        self.max_qtd_trades_running
                    )));
                }

                // Get market price corresponding to `timestamp`

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

                // Evaluate `takeprofit`
                let takeprofit = market_price.apply_gain(takeprofit_perc);

                // Evaluate `stoploss`
                let stoploss = market_price.apply_discount(stoploss_perc);

                // Create `SimulatedTradeRunning` and add it to `self`
                Ok(())
            }
            TradeOrder::OpenShort {
                timestamp,
                stoploss_perc,
                takeprofit_perc,
                balance_perc,
                leverage,
            } => Ok(()),
            TradeOrder::CloseLongs { timestamp } => Ok(()),
            TradeOrder::CloseShorts { timestamp } => Ok(()),
            TradeOrder::CloseAll { timestamp } => Ok(()),
        }
    }

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState> {
        let trades_state = TradesState::new(
            timestamp,
            self.long.len(),
            self.short.len(),
            self.closed.len(),
            None,
            None,
            self.balance,
            self.balance as i64 - self.start_balance as i64,
        );

        Ok(trades_state)
    }
}
