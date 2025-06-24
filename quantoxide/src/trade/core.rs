use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{
    DateTime, Utc,
    format::{DelayedFormat, StrftimeItems},
};
use futures::FutureExt;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Price, Trade, TradeSide,
    estimate_pl,
};

use crate::signal::core::Signal;

use super::error::{Result, TradeError};

#[derive(Debug, Clone)]
pub struct TradingState {
    current_time: DateTime<Utc>,
    current_balance: u64,
    market_price: f64,
    last_trade_time: Option<DateTime<Utc>>,
    running: Vec<Arc<dyn Trade>>,
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

impl TradingState {
    pub fn new(
        current_time: DateTime<Utc>,
        current_balance: u64,
        market_price: f64,
        last_trade_time: Option<DateTime<Utc>>,
        running: Vec<Arc<dyn Trade>>,
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
            current_time,
            current_balance,
            market_price,
            last_trade_time,
            running,
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

    pub fn current_time(&self) -> DateTime<Utc> {
        self.current_time
    }

    pub fn current_time_local(&self) -> DelayedFormat<StrftimeItems<'_>> {
        self.current_time
            .with_timezone(&chrono::Local)
            .format("%y-%m-%d %H:%M:%S%.3f %Z")
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

    pub fn last_trade_time_local(&self) -> Option<DelayedFormat<StrftimeItems<'_>>> {
        self.last_trade_time.map(|ltt| {
            ltt.with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M:%S%.3f %Z")
        })
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

    pub fn summary(&self) -> String {
        let mut result = String::new();

        result.push_str("timing:\n");
        result.push_str(&format!("  current_time: {}\n", self.current_time_local()));
        let lttl_str = self
            .last_trade_time_local()
            .map_or("null".to_string(), |lttl| lttl.to_string());
        result.push_str(&format!("  last_trade_time: {lttl_str}\n"));

        result.push_str("balance:\n");
        result.push_str(&format!("  current_balance: {}\n", self.current_balance));
        result.push_str(&format!("  market_price: {:.2}\n", self.market_price));

        result.push_str("running_positions:\n");
        result.push_str("  long:\n");
        result.push_str(&format!("    trades: {}\n", self.running_long_len));
        result.push_str(&format!("    margin: {}\n", self.running_long_margin));
        result.push_str(&format!("    quantity: {}\n", self.running_long_quantity));
        result.push_str("  short:\n");
        result.push_str(&format!("    trades: {}\n", self.running_short_len));
        result.push_str(&format!("    margin: {}\n", self.running_short_margin));
        result.push_str(&format!("    quantity: {}\n", self.running_short_quantity));

        result.push_str("running_metrics:\n");
        result.push_str(&format!("  pl: {}\n", self.running_pl));
        result.push_str(&format!("  fees: {}\n", self.running_fees));
        result.push_str(&format!(
            "  total_margin: {}\n",
            self.running_total_margin()
        ));

        result.push_str("closed_positions:\n");
        result.push_str(&format!("  trades: {}\n", self.closed_len));
        result.push_str(&format!("  pl: {}\n", self.closed_pl));
        result.push_str(&format!("  fees: {}", self.closed_fees));

        result
    }

    pub fn running_trades_table(&self) -> String {
        if self.running.is_empty() {
            return "No running trades".to_string();
        }

        let mut table = String::new();

        table.push_str(&format!(
            "{:>5} | {:>11} | {:>11} | {:>11} | {:>11} | {:>11} | {:>8} | {:>11} | {:>11} | {:>11}\n",
            "side",
            "quantity",
            "price",
            "liquidation",
            "stoploss",
            "takeprofit",
            "leverage",
            "margin",
            "pl",
            "fees"
        ));

        table.push_str(&format!("{}\n", "-".repeat(128)));

        for trade in &self.running {
            let stoploss_str = trade
                .stoploss()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let takeprofit_str = trade
                .takeprofit()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let pl = estimate_pl(
                trade.side(),
                trade.quantity(),
                trade.price(),
                Price::clamp_from(self.market_price()),
            );
            let total_fees = trade.opening_fee() + trade.closing_fee();

            table.push_str(&format!(
                "{:>5} | {:>11} | {:>11.1} | {:>11.1} | {:>11} | {:>11} | {:>8.2} | {:>11} | {:>11} | {:>11}\n",
                trade.side(),
                trade.quantity(),
                trade.price(),
                trade.liquidation(),
                stoploss_str,
                takeprofit_str,
                trade.leverage(),
                trade.margin(),
                pl,
                total_fees
            ));
        }

        table
    }
}

impl fmt::Display for TradingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TradingState:")?;
        for line in self.summary().lines() {
            write!(f, "\n  {line}")?;
        }
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
            Self::Short {
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TradeTrailingStoploss(BoundedPercentage);

impl TradeTrailingStoploss {
    pub fn new(tsl_step_size: BoundedPercentage, stoploss_perc: BoundedPercentage) -> Result<Self> {
        if tsl_step_size > stoploss_perc {
            return Err(TradeError::Generic(
                "`stoploss_perc` must be gt than `tsl_step_size`".to_string(),
            ));
        }

        Ok(Self(stoploss_perc))
    }
}

impl From<TradeTrailingStoploss> for BoundedPercentage {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0
    }
}

impl From<TradeTrailingStoploss> for LowerBoundedPercentage {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0.into()
    }
}

impl StoplossMode {
    pub(crate) fn validate_trade_tsl(
        self,
        tsl_step_size: BoundedPercentage,
        trade_sl: BoundedPercentage,
    ) -> Result<Option<TradeTrailingStoploss>> {
        match self {
            Self::Fixed => Ok(None),
            Self::Trailing => {
                let trade_tsl = TradeTrailingStoploss::new(tsl_step_size, trade_sl)?;
                Ok(Some(trade_tsl))
            }
        }
    }
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

    async fn trading_state(&self) -> Result<TradingState>;
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
    fn next_stoploss_update_trigger(
        &self,
        tsl_step_size: BoundedPercentage,
        trade_tsl: TradeTrailingStoploss,
    ) -> Result<Price> {
        if tsl_step_size > trade_tsl.into() {
            return Err(TradeError::Generic(
                "`tsl_step_size` cannot be gt than `trade_tsl`".to_string(),
            ));
        }

        let curr_stoploss = self
            .stoploss()
            .ok_or_else(|| TradeError::Generic("trade stoploss is not set".to_string()))?;

        let price_trigger = match self.side() {
            TradeSide::Buy => {
                let next_stoploss = curr_stoploss
                    .apply_gain(tsl_step_size.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                next_stoploss
                    .apply_gain(trade_tsl.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?
            }
            TradeSide::Sell => {
                let next_stoploss = curr_stoploss
                    .apply_discount(tsl_step_size)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                next_stoploss
                    .apply_discount(trade_tsl.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?
            }
        };

        Ok(price_trigger)
    }

    fn eval_trigger_bounds(
        &self,
        tsl_step_size: BoundedPercentage,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> Result<(Price, Price)> {
        let next_stoploss_update_trigger = trade_tsl
            .map(|tsl| self.next_stoploss_update_trigger(tsl_step_size, tsl))
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
                    .map_or(false, |stoploss| range_min <= stoploss.into_f64());
                let takeprofit_reached = self
                    .takeprofit()
                    .map_or(false, |takeprofit| range_max >= takeprofit.into_f64());

                return stoploss_reached || takeprofit_reached;
            }
            TradeSide::Sell => {
                let stoploss_reached = self
                    .stoploss()
                    .map_or(false, |stoploss| range_max >= stoploss.into_f64());
                let takeprofit_reached = self
                    .takeprofit()
                    .map_or(false, |takeprofit| range_min <= takeprofit.into_f64());

                return stoploss_reached || takeprofit_reached;
            }
        };
    }

    fn eval_new_stoploss_on_range(
        &self,
        tsl_step_size: BoundedPercentage,
        trade_tsl: TradeTrailingStoploss,
        range_min: f64,
        range_max: f64,
    ) -> Result<Option<Price>> {
        let next_stoploss_update_trigger = self
            .next_stoploss_update_trigger(tsl_step_size, trade_tsl)?
            .into_f64();

        let new_stoploss = match self.side() {
            TradeSide::Buy => {
                if range_max >= next_stoploss_update_trigger {
                    let new_stoploss = Price::round(range_max)
                        .map_err(|e| TradeError::Generic(e.to_string()))?
                        .apply_discount(trade_tsl.into())
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
                        .apply_gain(trade_tsl.into())
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
        tsl_step_size: BoundedPercentage,
        trade: &impl TradeExt,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> Result<()> {
        let (mut new_min, mut new_max) = trade.eval_trigger_bounds(tsl_step_size, trade_tsl)?;

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
