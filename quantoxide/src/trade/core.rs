use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;
use lazy_static::lazy_static;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Price, Trade, TradeSide,
};

use crate::signal::core::Signal;

use super::error::{Result, TradeError};

lazy_static! {
    static ref TRAILING_STOPLOSS_PERC_TICK: BoundedPercentage =
        BoundedPercentage::try_from(0.1).expect("is valid `BoundedPercentage`");
}

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
    running_long_quantity: u64,
    running_short_len: usize,
    running_short_margin: u64,
    running_short_quantity: u64,
    running_pl: i64,
    running_fees: u64,
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
        running_long_quantity: u64,
        running_short_len: usize,
        running_short_margin: u64,
        running_short_quantity: u64,
        running_pl: i64,
        running_fees: u64,
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
            running_long_quantity,
            running_short_len,
            running_short_margin,
            running_short_quantity,
            running_pl,
            running_fees,
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

    pub fn running_long_quantity(&self) -> u64 {
        self.running_long_quantity
    }

    /// Returns the number of running short trades
    pub fn running_short_len(&self) -> usize {
        self.running_short_len
    }

    /// Returns the locked margin for short positions, if available
    pub fn running_short_margin(&self) -> u64 {
        self.running_short_margin
    }

    pub fn running_short_quantity(&self) -> u64 {
        self.running_short_quantity
    }

    /// Returns the number of running trades
    pub fn running_len(&self) -> usize {
        self.running_long_len + self.running_short_len
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

    pub fn running_total_margin(&self) -> u64 {
        self.running_long_margin + self.running_short_margin
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
        writeln!(f, "    market_price: {:.2}", self.market_price)?;

        writeln!(f, "  running_positions:")?;
        writeln!(f, "    long:")?;
        writeln!(f, "      trades: {}", self.running_long_len)?;
        writeln!(f, "      margin: {}", self.running_long_margin)?;
        writeln!(f, "      quantity: {}", self.running_long_quantity)?;
        writeln!(f, "    short:")?;
        writeln!(f, "      trades: {}", self.running_short_len)?;
        writeln!(f, "      margin: {}", self.running_short_margin)?;
        writeln!(f, "      quantity: {}", self.running_short_quantity)?;

        writeln!(f, "  running_metrics:")?;
        writeln!(f, "    pl: {}", self.running_pl)?;
        writeln!(f, "    fees: {}", self.running_fees)?;
        writeln!(f, "    total_margin: {}", self.running_total_margin())?;

        writeln!(f, "  closed_positions:")?;
        writeln!(f, "    trades: {}", self.closed_len)?;
        writeln!(f, "    pl: {}", self.closed_pl)?;
        write!(f, "    fees: {}", self.closed_fees)?;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoplossMode {
    Fixed,
    Trailing,
}

#[async_trait]
pub trait TradeController: Send + Sync {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()>;

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
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
        trade_controller: Arc<dyn TradeController>,
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
        trade_controller: Arc<dyn TradeController>,
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

pub trait TradeExt: Trade {
    fn next_stoploss_update_trigger(&self, trailing_stoploss: BoundedPercentage) -> Result<Price> {
        let curr_stoploss = self
            .stoploss()
            .ok_or_else(|| TradeError::Generic("trade stoploss is not set".to_string()))?;

        match self.side() {
            TradeSide::Buy => {
                let next_stoploss = curr_stoploss
                    .apply_gain(TRAILING_STOPLOSS_PERC_TICK.clone().into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                let price_trigger = next_stoploss
                    .apply_gain(trailing_stoploss.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                Ok(price_trigger)
            }
            TradeSide::Sell => {
                let next_stoploss = curr_stoploss
                    .apply_discount(TRAILING_STOPLOSS_PERC_TICK.clone())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                let price_trigger = next_stoploss
                    .apply_discount(trailing_stoploss.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                Ok(price_trigger)
            }
        }
    }

    fn eval_trigger_bounds(
        &self,
        trailing_stoploss: Option<BoundedPercentage>,
    ) -> Result<(Price, Price)> {
        let next_stoploss_update_trigger = trailing_stoploss
            .map(|tsl| self.next_stoploss_update_trigger(tsl))
            .transpose()?;

        match self.side() {
            TradeSide::Buy => {
                let lower_bound = self.stoploss().unwrap_or(Price::MIN);

                let takeprofit_trigger = self.takeprofit().unwrap_or(Price::MAX);
                let upper_bound =
                    takeprofit_trigger.min(next_stoploss_update_trigger.unwrap_or(Price::MAX));

                return Ok((lower_bound, upper_bound));
            }

            TradeSide::Sell => {
                let takeprofit_trigger = self.takeprofit().unwrap_or(Price::MIN);
                let lower_bound =
                    takeprofit_trigger.max(next_stoploss_update_trigger.unwrap_or(Price::MIN));

                let upper_bound = self.stoploss().unwrap_or(Price::MAX);

                return Ok((lower_bound, upper_bound));
            }
        };
    }

    fn was_closed_on_range(&self, range_min: f64, range_max: f64) -> bool {
        match self.side() {
            TradeSide::Buy => {
                let stoploss_reached = self
                    .stoploss()
                    .map_or(false, |stoploss| stoploss.into_f64() >= range_min);
                let takeprofit_reached = self
                    .takeprofit()
                    .map_or(false, |takeprofit| takeprofit.into_f64() <= range_max);

                return stoploss_reached || takeprofit_reached;
            }
            TradeSide::Sell => {
                let stoploss_reached = self
                    .stoploss()
                    .map_or(false, |stoploss| stoploss.into_f64() >= range_max);
                let takeprofit_reached = self
                    .takeprofit()
                    .map_or(false, |takeprofit| takeprofit.into_f64() <= range_min);

                return stoploss_reached || takeprofit_reached;
            }
        };
    }

    fn eval_new_stoploss_on_range(
        &self,
        range_min: f64,
        range_max: f64,
        trailing_stoploss: BoundedPercentage,
    ) -> Result<Option<Price>> {
        let next_stoploss_update_trigger = self
            .next_stoploss_update_trigger(trailing_stoploss)?
            .into_f64();

        let new_stoploss = match self.side() {
            TradeSide::Buy => {
                if range_max >= next_stoploss_update_trigger {
                    let new_stoploss = Price::round(range_max)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .apply_discount(trailing_stoploss)
                        .map_err(|e| TradeError::Generic(e.to_string()))?;

                    Some(new_stoploss)
                } else {
                    None
                }
            }
            TradeSide::Sell => {
                if range_min <= next_stoploss_update_trigger {
                    let new_stoploss = Price::round(range_min)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .apply_gain(trailing_stoploss.into())
                        .map_err(|e| TradeError::Generic(e.to_string()))?;

                    Some(new_stoploss)
                } else {
                    None
                }
            }
        };

        Ok(new_stoploss)
    }
}

impl TradeExt for LnmTrade {}

#[derive(Debug, Clone)]
pub enum PriceTrigger {
    NotSet,
    Set { min: Price, max: Price },
}

impl PriceTrigger {
    pub fn new() -> Self {
        Self::NotSet
    }

    pub fn update(
        &mut self,
        trade: &impl TradeExt,
        trailing_stoploss: Option<BoundedPercentage>,
    ) -> Result<()> {
        let (mut new_min, mut new_max) = trade.eval_trigger_bounds(trailing_stoploss)?;

        if let PriceTrigger::Set { min, max } = *self {
            new_min = new_min.max(min);
            new_max = new_max.min(max);
        }

        *self = PriceTrigger::Set {
            min: new_min,
            max: new_max,
        };

        Ok(())
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
