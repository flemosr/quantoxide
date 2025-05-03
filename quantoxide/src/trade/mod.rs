use async_trait::async_trait;
use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage, Margin};

mod error;
mod simulation;

use error::Result;

pub use simulation::SimulatedTradesManager;

pub struct TradesState {
    start_time: DateTime<Utc>,
    start_balance: u64,
    current_time: DateTime<Utc>,
    current_balance: u64,
    running_long_qtd: usize,
    running_long_margin: Option<Margin>,
    running_short_qtd: usize,
    running_short_margin: Option<Margin>,
    running_pl: i64,
    running_fees_est: u64,
    closed_qtd: usize,
    closed_pl: i64,
    closed_fees: u64,
}

impl TradesState {
    fn new(
        start_time: DateTime<Utc>,
        start_balance: u64,
        current_time: DateTime<Utc>,
        current_balance: u64,
        running_long_qtd: usize,
        running_long_margin: Option<Margin>,
        running_short_qtd: usize,
        running_short_margin: Option<Margin>,
        running_pl: i64,
        running_fees_est: u64,
        closed_qtd: usize,
        closed_pl: i64,
        closed_fees: u64,
    ) -> Self {
        Self {
            start_time,
            start_balance,
            current_time,
            current_balance,
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            running_fees_est,
            closed_qtd,
            closed_pl,
            closed_fees,
        }
    }

    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    pub fn start_balance(&self) -> u64 {
        self.start_balance
    }

    pub fn current_time(&self) -> DateTime<Utc> {
        self.current_time
    }

    pub fn current_balance(&self) -> u64 {
        self.current_balance
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

    pub fn running_pl(&self) -> i64 {
        self.running_pl
    }

    pub fn running_fees_est(&self) -> u64 {
        self.running_fees_est
    }

    pub fn running_net_pl(&self) -> i64 {
        self.running_pl - self.running_fees_est as i64
    }

    pub fn closed_pl(&self) -> i64 {
        self.closed_pl
    }

    pub fn closed_fees(&self) -> u64 {
        self.closed_fees
    }

    pub fn closed_net_pl(&self) -> i64 {
        self.closed_pl - self.closed_fees as i64
    }

    pub fn pl(&self) -> i64 {
        self.running_pl + self.closed_pl
    }

    pub fn fees(&self) -> u64 {
        self.running_fees_est + self.closed_fees
    }

    pub fn net_pl(&self) -> i64 {
        self.pl() - self.fees() as i64
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

    async fn state(&self) -> Result<TradesState>;
}
