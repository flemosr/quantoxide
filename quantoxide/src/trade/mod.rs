use async_trait::async_trait;
use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage, Margin};

mod error;
mod simulated;

use error::Result;

pub struct TradesState {
    timestamp: DateTime<Utc>,
    qtd_trades_running_long: usize,
    qtd_trades_running_short: usize,
    qtd_trades_closed: usize,
    locked_margin_long: Option<Margin>,
    locked_margin_short: Option<Margin>,
    balance: u64,
    running_pl: i64,
    closed_pl: i64,
}

impl TradesState {
    fn new(
        timestamp: DateTime<Utc>,
        qtd_trades_running_long: usize,
        qtd_trades_running_short: usize,
        qtd_trades_closed: usize,
        locked_margin_long: Option<Margin>,
        locked_margin_short: Option<Margin>,
        balance: u64,
        running_pl: i64,
        closed_pl: i64,
    ) -> Self {
        Self {
            timestamp,
            qtd_trades_running_long,
            qtd_trades_running_short,
            qtd_trades_closed,
            locked_margin_long,
            locked_margin_short,
            balance,
            running_pl,
            closed_pl,
        }
    }

    /// Returns the timestamp of this trade state
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the quantity of running long trades
    pub fn qtd_trades_running_long(&self) -> usize {
        self.qtd_trades_running_long
    }

    /// Returns the quantity of running short trades
    pub fn qtd_trades_running_short(&self) -> usize {
        self.qtd_trades_running_short
    }

    /// Returns the quantity of running long and short trades
    pub fn qtd_trades_running(&self) -> usize {
        self.qtd_trades_running_long + self.qtd_trades_running_short
    }

    /// Returns the quantity of closed trades
    pub fn qtd_trades_closed(&self) -> usize {
        self.qtd_trades_closed
    }

    /// Returns the locked margin for long positions, if available
    pub fn locked_margin_long(&self) -> Option<Margin> {
        self.locked_margin_long
    }

    /// Returns the locked margin for short positions, if available
    pub fn locked_margin_short(&self) -> Option<Margin> {
        self.locked_margin_short
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn running_pl(&self) -> i64 {
        self.running_pl
    }

    pub fn closed_pl(&self) -> i64 {
        self.closed_pl
    }

    pub fn total_pl(&self) -> i64 {
        self.running_pl + self.closed_pl
    }
}

#[async_trait]
pub trait TradesManager {
    async fn open_long(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()>;

    async fn open_short(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()>;

    async fn close_longs(&self, timestamp: DateTime<Utc>) -> Result<()>;

    async fn close_shorts(&self, timestamp: DateTime<Utc>) -> Result<()>;

    async fn close_all(&self, timestamp: DateTime<Utc>) -> Result<()>;

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState>;
}
