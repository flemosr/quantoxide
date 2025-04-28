use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

use lnm_sdk::api::rest::models::{Leverage, Price, Quantity};

use crate::db::DbContext;

use super::{TradeOrder, TradesManager, TradesState, error::Result};

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
    start_balance: u64,
    balance: u64,
    time: DateTime<Utc>,
    long: Vec<SimulatedTradeRunning>,
    short: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
}

impl SimulatedTradesManager {
    pub fn new(db: Arc<DbContext>, start_balance: u64, start_time: DateTime<Utc>) -> Self {
        Self {
            db,
            start_balance,
            balance: start_balance,
            time: start_time,
            long: Vec::new(),
            short: Vec::new(),
            closed: Vec::new(),
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
            } => Ok(()),
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
