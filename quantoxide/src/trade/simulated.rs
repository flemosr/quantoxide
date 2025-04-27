use chrono::{DateTime, Utc};
use std::sync::Arc;

use lnm_sdk::api::rest::models::{Leverage, Price, Quantity};

use crate::db::DbContext;

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
    balance: u64,
    time: DateTime<Utc>,
    long: Vec<SimulatedTradeRunning>,
    short: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
}
