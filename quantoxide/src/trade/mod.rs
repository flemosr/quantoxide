use async_trait::async_trait;
use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage, Margin};

mod error;
mod simulated;

use error::Result;

pub struct TradesState {
    time: DateTime<Utc>,
    balance: u64,
    running_long_qtd: usize,
    running_long_margin: Option<Margin>,
    running_short_qtd: usize,
    running_short_margin: Option<Margin>,
    running_pl: i64,
    closed_qtd: usize,
    closed_pl: i64,
}

impl TradesState {
    fn new(
        time: DateTime<Utc>,
        balance: u64,
        running_long_qtd: usize,
        running_long_margin: Option<Margin>,
        running_short_qtd: usize,
        running_short_margin: Option<Margin>,
        running_pl: i64,
        closed_qtd: usize,
        closed_pl: i64,
    ) -> Self {
        Self {
            time,
            balance,
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            closed_qtd,
            closed_pl,
        }
    }

    /// Returns the timestamp of this trade state
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.time
    }

    /// Returns the quantity of running long trades
    pub fn running_long_qtd(&self) -> usize {
        self.running_long_qtd
    }

    /// Returns the locked margin for long positions, if available
    pub fn running_long_margin(&self) -> Option<Margin> {
        self.running_long_margin
    }

    /// Returns the quantity of running short trades
    pub fn running_short_qtd(&self) -> usize {
        self.running_short_qtd
    }

    /// Returns the locked margin for short positions, if available
    pub fn running_short_margin(&self) -> Option<Margin> {
        self.running_short_margin
    }

    /// Returns the quantity of running long and short trades
    pub fn running_qtd(&self) -> usize {
        self.running_long_qtd + self.running_short_qtd
    }

    pub fn running_margin(&self) -> Option<Margin> {
        match (self.running_long_margin, self.running_short_margin) {
            (Some(long_margin), Some(short_margin)) => Some(long_margin + short_margin),
            _ => self.running_long_margin.or(self.running_short_margin),
        }
    }

    /// Returns the quantity of closed trades
    pub fn closed_qtd(&self) -> usize {
        self.closed_qtd
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

    pub fn pl(&self) -> i64 {
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
