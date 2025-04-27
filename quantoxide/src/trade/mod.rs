use async_trait::async_trait;
use chrono::{DateTime, Utc};

mod error;

use error::{Result, TradeError};
use lnm_sdk::api::rest::models::Margin;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct StoplossPerc(f32);

impl TryFrom<f32> for StoplossPerc {
    type Error = TradeError;

    fn try_from(value: f32) -> Result<Self> {
        if value < 0.1 || value >= 99.9 {
            return Err(TradeError::Generic(format!(
                "`StoplossPerc` must be gte 0.1 and lte 99.9, got {value}"
            )));
        }
        Ok(Self(value))
    }
}

impl From<StoplossPerc> for f32 {
    fn from(perc: StoplossPerc) -> f32 {
        perc.0
    }
}

impl Eq for StoplossPerc {}

impl Ord for StoplossPerc {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TakeprofitPerc(f32);

impl TryFrom<f32> for TakeprofitPerc {
    type Error = TradeError;

    fn try_from(value: f32) -> Result<Self> {
        if value < 0.1 {
            return Err(TradeError::Generic(format!(
                "`TakeprofitPerc` must be gte 0.1, got {value}"
            )));
        }
        if value == f32::INFINITY {
            return Err(TradeError::Generic(format!(
                "`TakeprofitPerc` must be finite"
            )));
        }

        Ok(Self(value))
    }
}

impl From<TakeprofitPerc> for f32 {
    fn from(perc: TakeprofitPerc) -> f32 {
        perc.0
    }
}

impl Eq for TakeprofitPerc {}

impl Ord for TakeprofitPerc {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

pub enum TradeOrder {
    OpenLong {
        timestamp: DateTime<Utc>,
        stoploss_perc: StoplossPerc,
        takeprofit_perc: TakeprofitPerc,
    },
    OpenShort {
        timestamp: DateTime<Utc>,
        stoploss_perc: StoplossPerc,
        takeprofit_perc: TakeprofitPerc,
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
        stoploss_perc: StoplossPerc,
        takeprofit_perc: TakeprofitPerc,
    ) -> Self {
        Self::OpenLong {
            timestamp,
            stoploss_perc,
            takeprofit_perc,
        }
    }

    pub fn open_short(
        timestamp: DateTime<Utc>,
        stoploss_perc: StoplossPerc,
        takeprofit_perc: TakeprofitPerc,
    ) -> Self {
        Self::OpenShort {
            timestamp,
            stoploss_perc,
            takeprofit_perc,
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
    balance: u64,
    pl: i64,
}

impl TradesState {
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
