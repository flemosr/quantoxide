use async_trait::async_trait;
use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{Leverage, Margin};

mod error;
mod simulated;

use error::{Result, TradeError};

/// Represents a percentage value that is constrained within a specific range.
///
/// This struct wraps an f32 value that must be:
/// - Greater than or equal to 0.1%
/// - Less than or equal to 99.9%
///
/// This bounded range makes it suitable for percentage calculations where both
/// minimum and maximum limits are required.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BoundedPercentage(f32);

impl TryFrom<f32> for BoundedPercentage {
    type Error = TradeError;

    fn try_from(value: f32) -> Result<Self> {
        if value < 0.1 || value > 99.9 {
            return Err(TradeError::Generic(format!(
                "`BoundedPercentage` must be gte 0.1 and lte 99.9, got {value}"
            )));
        }
        Ok(Self(value))
    }
}

impl From<BoundedPercentage> for f32 {
    fn from(perc: BoundedPercentage) -> f32 {
        perc.0
    }
}

impl Eq for BoundedPercentage {}

impl Ord for BoundedPercentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("`BoundedPercentage` must be finite")
    }
}

/// Represents a percentage value that is only constrained by a lower bound.
///
/// This struct wraps an f32 value that must be:
/// - Greater than or equal to 0.1%
/// - Finite (not infinity)
///
/// This type is suitable for percentage calculations where only a minimum
/// threshold is needed, with no practical upper limit other than it must be a
/// finite value.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct LowerBoundedPercentage(f32);

impl TryFrom<f32> for LowerBoundedPercentage {
    type Error = TradeError;

    fn try_from(value: f32) -> Result<Self> {
        if value < 0.1 {
            return Err(TradeError::Generic(format!(
                "`LowerBoundedPercentage` must be gte 0.1, got {value}"
            )));
        }
        if value == f32::INFINITY {
            return Err(TradeError::Generic(format!(
                "`LowerBoundedPercentage` must be finite"
            )));
        }

        Ok(Self(value))
    }
}

impl From<LowerBoundedPercentage> for f32 {
    fn from(perc: LowerBoundedPercentage) -> f32 {
        perc.0
    }
}

impl Eq for LowerBoundedPercentage {}

impl Ord for LowerBoundedPercentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("`LowerBoundedPercentage` must be finite")
    }
}

pub enum TradeOrder {
    OpenLong {
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    },
    OpenShort {
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    },
    CloseLongs {
        timestamp: DateTime<Utc>,
    },
    CloseShorts {
        timestamp: DateTime<Utc>,
    },
    CloseAll {
        timestamp: DateTime<Utc>,
    },
}

impl TradeOrder {
    pub fn open_long(
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Self {
        Self::OpenLong {
            timestamp,
            stoploss_perc,
            takeprofit_perc,
            balance_perc,
            leverage,
        }
    }

    pub fn open_short(
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Self {
        Self::OpenShort {
            timestamp,
            stoploss_perc,
            takeprofit_perc,
            balance_perc,
            leverage,
        }
    }

    pub fn close_longs(timestamp: DateTime<Utc>) -> Self {
        Self::CloseLongs { timestamp }
    }

    pub fn close_shorts(timestamp: DateTime<Utc>) -> Self {
        Self::CloseShorts { timestamp }
    }

    pub fn close_all(timestamp: DateTime<Utc>) -> Self {
        Self::CloseAll { timestamp }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            TradeOrder::OpenLong { timestamp, .. } => *timestamp,
            TradeOrder::OpenShort { timestamp, .. } => *timestamp,
            TradeOrder::CloseLongs { timestamp } => *timestamp,
            TradeOrder::CloseShorts { timestamp } => *timestamp,
            TradeOrder::CloseAll { timestamp } => *timestamp,
        }
    }
}

pub struct TradesState {
    timestamp: DateTime<Utc>,
    locked_margin_long: Margin,
    locked_margin_short: Margin,
    qtd_trades_long: usize,
    qtd_trades_short: usize,
    qtd_trades_closed: usize,
    balance: u64,
    pl: i64,
}

impl TradesState {
    fn new(
        timestamp: DateTime<Utc>,
        locked_margin_long: Margin,
        locked_margin_short: Margin,
        qtd_trades_long: usize,
        qtd_trades_short: usize,
        qtd_trades_closed: usize,
        balance: u64,
        pl: i64,
    ) -> Self {
        Self {
            timestamp,
            locked_margin_long,
            locked_margin_short,
            qtd_trades_long,
            qtd_trades_short,
            qtd_trades_closed,
            balance,
            pl,
        }
    }

    /// Returns the timestamp of this trade state
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the locked margin for long positions
    pub fn locked_margin_long(&self) -> Margin {
        self.locked_margin_long
    }

    /// Returns the locked margin for short positions
    pub fn locked_margin_short(&self) -> Margin {
        self.locked_margin_short
    }

    /// Returns the quantity of open long trades
    pub fn qtd_trades_long(&self) -> usize {
        self.qtd_trades_long
    }

    /// Returns the quantity of open short trades
    pub fn qtd_trades_short(&self) -> usize {
        self.qtd_trades_short
    }

    /// Returns the quantity of closed trades
    pub fn qtd_trades_closed(&self) -> usize {
        self.qtd_trades_closed
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    /// Returns the profit/loss
    pub fn pl(&self) -> i64 {
        self.pl
    }
}

#[async_trait]
pub trait TradesManager {
    async fn order(&self, order: TradeOrder) -> Result<()>;

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState>;
}
