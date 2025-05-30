use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Trade, TradeSide,
};

use crate::signal::core::Signal;

use super::error::{Result, TradeError};

#[derive(Debug, Clone, PartialEq)]
pub struct TradeControllerState {
    start_time: DateTime<Utc>,
    start_balance: u64,
    current_time: DateTime<Utc>,
    current_balance: u64,
    market_price: f64,
    last_trade_time: Option<DateTime<Utc>>,
    running_long_len: usize,
    running_long_margin: u64,
    running_short_len: usize,
    running_short_margin: u64,
    running_pl: i64,
    running_fees: u64,
    running_maintenance_margin: u64,
    closed_len: usize,
    closed_pl: i64,
    closed_fees: u64,
}

impl TradeControllerState {
    pub fn new(
        start_time: DateTime<Utc>,
        start_balance: u64,
        current_time: DateTime<Utc>,
        current_balance: u64,
        market_price: f64,
        last_trade_time: Option<DateTime<Utc>>,
        running_long_len: usize,
        running_long_margin: u64,
        running_short_len: usize,
        running_short_margin: u64,
        running_pl: i64,
        running_fees: u64,
        running_maintenance_margin: u64,
        closed_len: usize,
        closed_pl: i64,
        closed_fees: u64,
    ) -> Self {
        Self {
            start_time,
            start_balance,
            current_time,
            current_balance,
            market_price,
            last_trade_time,
            running_long_len,
            running_long_margin,
            running_short_len,
            running_short_margin,
            running_pl,
            running_fees,
            running_maintenance_margin,
            closed_len,
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

    pub fn market_price(&self) -> f64 {
        self.market_price
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.last_trade_time
    }

    /// Returns the number of running long trades
    pub fn running_long_len(&self) -> usize {
        self.running_long_len
    }

    /// Returns the locked margin for long positions, if available
    pub fn running_long_margin(&self) -> u64 {
        self.running_long_margin
    }

    /// Returns the number of running short trades
    pub fn running_short_len(&self) -> usize {
        self.running_short_len
    }

    /// Returns the locked margin for short positions, if available
    pub fn running_short_margin(&self) -> u64 {
        self.running_short_margin
    }

    /// Returns the number of running trades
    pub fn running_len(&self) -> usize {
        self.running_long_len + self.running_short_len
    }

    pub fn running_margin(&self) -> u64 {
        self.running_long_margin + self.running_short_margin
    }

    /// Returns the number of closed trades
    pub fn closed_len(&self) -> usize {
        self.closed_len
    }

    pub fn running_pl(&self) -> i64 {
        self.running_pl
    }

    pub fn running_fees(&self) -> u64 {
        self.running_fees
    }

    pub fn running_maintenance_margin(&self) -> u64 {
        self.running_maintenance_margin
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

    pub fn fees_estimated(&self) -> u64 {
        self.running_fees + self.closed_fees
    }

    pub fn net_pl_estimated(&self) -> i64 {
        self.pl() - self.fees_estimated() as i64
    }
}

impl fmt::Display for TradeControllerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "TradeControllerState:")?;

        writeln!(f, "  timing:")?;
        writeln!(f, "    start_time: {}", self.start_time.to_rfc3339())?;
        writeln!(f, "    current_time: {}", self.current_time.to_rfc3339())?;
        match self.last_trade_time {
            Some(time) => writeln!(f, "    last_trade_time: {}", time.to_rfc3339())?,
            None => writeln!(f, "    last_trade_time: null")?,
        }

        writeln!(f, "  balance:")?;
        writeln!(f, "    start_balance: {}", self.start_balance)?;
        writeln!(f, "    current_balance: {}", self.current_balance)?;
        writeln!(f, "    market_price: {:.6}", self.market_price)?;

        writeln!(f, "  running_positions:")?;
        writeln!(f, "    long:")?;
        writeln!(f, "      trades: {}", self.running_long_len)?;
        writeln!(f, "      margin: {}", self.running_long_margin)?;
        writeln!(f, "    short:")?;
        writeln!(f, "      trades: {}", self.running_short_len)?;
        writeln!(f, "      margin: {}", self.running_short_margin)?;

        writeln!(f, "  running_metrics:")?;
        writeln!(f, "    pl: {}", self.running_pl)?;
        writeln!(f, "    fees: {}", self.running_fees)?;
        writeln!(
            f,
            "    maintenance_margin: {}",
            self.running_maintenance_margin
        )?;

        writeln!(f, "  closed_positions:")?;
        writeln!(f, "    trades: {}", self.closed_len)?;
        writeln!(f, "    pl: {}", self.closed_pl)?;
        writeln!(f, "    fees: {}", self.closed_fees)?;

        Ok(())
    }
}

pub enum RiskParams {
    Long {
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
    },
    Short {
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
    },
}

impl RiskParams {
    pub fn into_trade_params(self, market_price: Price) -> Result<(TradeSide, Price, Price)> {
        match self {
            Self::Long {
                stoploss_perc,
                takeprofit_perc,
            } => {
                let stoploss = market_price
                    .apply_discount(stoploss_perc)
                    .map_err(TradeError::RiskParamsConversion)?;
                let takeprofit = market_price
                    .apply_gain(takeprofit_perc.into())
                    .map_err(TradeError::RiskParamsConversion)?;

                Ok((TradeSide::Buy, stoploss, takeprofit))
            }
            RiskParams::Short {
                stoploss_perc,
                takeprofit_perc,
            } => {
                let stoploss = market_price
                    .apply_gain(stoploss_perc.into())
                    .map_err(TradeError::RiskParamsConversion)?;
                let takeprofit = market_price
                    .apply_discount(takeprofit_perc)
                    .map_err(TradeError::RiskParamsConversion)?;

                Ok((TradeSide::Sell, stoploss, takeprofit))
            }
        }
    }
}

#[async_trait]
pub trait TradeController {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()>;

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()>;

    async fn close_longs(&self) -> Result<()>;

    async fn close_shorts(&self) -> Result<()>;

    async fn close_all(&self) -> Result<()>;

    async fn state(&self) -> Result<TradeControllerState>;
}

#[async_trait]
pub trait Operator: Send + Sync {
    fn set_trade_controller(
        &mut self,
        trade_controller: Arc<dyn TradeController + Send + Sync>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;

    async fn process_signal(
        &self,
        signal: &Signal,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
}

pub(crate) struct WrappedOperator(Box<dyn Operator>);

impl WrappedOperator {
    pub fn set_trade_controller(
        &mut self,
        trade_controller: Arc<dyn TradeController + Send + Sync>,
    ) -> Result<()> {
        panic::catch_unwind(AssertUnwindSafe(|| {
            self.0.set_trade_controller(trade_controller)
        }))
        .map_err(|_| TradeError::Generic(format!("`Operator::set_trade_controller` panicked")))?
        .map_err(|e| {
            TradeError::Generic(format!(
                "`Operator::set_trade_controller` error {}",
                e.to_string()
            ))
        })
    }

    pub async fn process_signal(&self, signal: &Signal) -> Result<()> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.process_signal(signal)))
            .await
            .map_err(|_| TradeError::Generic(format!("`Operator::consume_signal` panicked")))?
            .map_err(|e| {
                TradeError::Generic(format!(
                    "`Operator::consume_signal`  error {}",
                    e.to_string()
                ))
            })
    }
}

impl From<Box<dyn Operator>> for WrappedOperator {
    fn from(value: Box<dyn Operator>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone)]
pub enum PriceTrigger {
    NotSet,
    Set { min: Price, max: Price },
}

impl PriceTrigger {
    pub fn new() -> Self {
        Self::NotSet
    }

    pub fn update(&mut self, trade: &impl Trade) {
        let (mut new_min, mut new_max) = match (trade.stoploss(), trade.takeprofit()) {
            (None, None) => return,
            (Some(sl), None) => match trade.side() {
                TradeSide::Buy => (sl, Price::MAX),
                TradeSide::Sell => (Price::MIN, sl),
            },
            (None, Some(tp)) => match trade.side() {
                TradeSide::Buy => (Price::MIN, tp),
                TradeSide::Sell => (tp, Price::MAX),
            },
            (Some(sl), Some(tp)) => (sl.min(tp), sl.max(tp)),
        };

        if let PriceTrigger::Set { min, max } = *self {
            new_min = new_min.max(min);
            new_max = new_max.min(max);
        }

        *self = PriceTrigger::Set {
            min: new_min,
            max: new_max,
        };
    }

    pub fn was_reached(&self, market_price: f64) -> bool {
        match self {
            PriceTrigger::NotSet => false,
            PriceTrigger::Set { min, max } => {
                market_price <= min.into_f64() || market_price >= max.into_f64()
            }
        }
    }
}
