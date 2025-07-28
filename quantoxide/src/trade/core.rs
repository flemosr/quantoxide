use std::{
    cell::OnceCell,
    collections::{BTreeMap, HashMap},
    fmt,
    num::NonZeroU64,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;
use uuid::Uuid;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Price, Trade, TradeClosed,
    TradeRunning, TradeSide, TradeSize,
};

use crate::{signal::core::Signal, util::DateTimeExt};

use super::error::{Result, TradeError};

#[derive(Debug, Clone)]
struct RunningStats {
    long_len: usize,
    long_margin: u64,
    long_quantity: u64,
    short_len: usize,
    short_margin: u64,
    short_quantity: u64,
    pl: i64,
    fees: u64,
}

#[derive(Debug, Clone)]
pub struct TradingState {
    last_tick_time: DateTime<Utc>,
    balance: u64,
    market_price: Price,
    last_trade_time: Option<DateTime<Utc>>,
    running: HashMap<Uuid, (Arc<dyn TradeRunning>, Option<TradeTrailingStoploss>)>,
    running_stats: OnceCell<RunningStats>,
    running_sorted: OnceCell<Vec<(Arc<dyn TradeRunning>, Option<TradeTrailingStoploss>)>>,
    closed_len: usize,
    closed_pl: i64,
    closed_fees: u64,
}

impl TradingState {
    pub(crate) fn new(
        last_tick_time: DateTime<Utc>,
        balance: u64,
        market_price: Price,
        last_trade_time: Option<DateTime<Utc>>,
        running: HashMap<Uuid, (Arc<dyn TradeRunning>, Option<TradeTrailingStoploss>)>,
        closed_len: usize,
        closed_pl: i64,
        closed_fees: u64,
    ) -> Self {
        Self {
            last_tick_time,
            balance,
            market_price,
            last_trade_time,
            running,
            running_stats: OnceCell::new(),
            running_sorted: OnceCell::new(),
            closed_len,
            closed_pl,
            closed_fees,
        }
    }

    fn get_running_stats(&self) -> &RunningStats {
        self.running_stats.get_or_init(|| {
            let mut long_len = 0;
            let mut long_margin = 0;
            let mut long_quantity = 0;
            let mut short_len = 0;
            let mut short_margin = 0;
            let mut short_quantity = 0;
            let mut pl = 0;
            let mut fees = 0;

            for (trade, _) in self.running.values() {
                match trade.side() {
                    TradeSide::Buy => {
                        long_len += 1;
                        long_margin +=
                            trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                        long_quantity += trade.quantity().into_u64();
                    }
                    TradeSide::Sell => {
                        short_len += 1;
                        short_margin +=
                            trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                        short_quantity += trade.quantity().into_u64();
                    }
                }
                pl += trade.est_pl(self.market_price);
                fees += trade.opening_fee();
            }

            RunningStats {
                long_len,
                long_margin,
                long_quantity,
                short_len,
                short_margin,
                short_quantity,
                pl,
                fees,
            }
        })
    }

    fn get_running_sorted(&self) -> &Vec<(Arc<dyn TradeRunning>, Option<TradeTrailingStoploss>)> {
        self.running_sorted.get_or_init(|| {
            let mut running_vec = Vec::with_capacity(self.running.len());

            for (trade, tsl) in self.running.values() {
                running_vec.push((trade.clone(), *tsl));
            }

            running_vec.sort_by(|a, b| b.0.creation_ts().cmp(&a.0.creation_ts()));
            running_vec
        })
    }

    pub fn last_tick_time(&self) -> DateTime<Utc> {
        self.last_tick_time
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn market_price(&self) -> Price {
        self.market_price
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.last_trade_time
    }

    pub fn running(
        &self,
    ) -> &HashMap<Uuid, (Arc<dyn TradeRunning>, Option<TradeTrailingStoploss>)> {
        &self.running
    }

    pub fn running_long_len(&self) -> usize {
        self.get_running_stats().long_len
    }

    /// Returns the locked margin for long positions, if available
    pub fn running_long_margin(&self) -> u64 {
        self.get_running_stats().long_margin
    }

    pub fn running_long_quantity(&self) -> u64 {
        self.get_running_stats().long_quantity
    }

    pub fn running_short_len(&self) -> usize {
        self.get_running_stats().short_len
    }

    /// Returns the locked margin for short positions, if available
    pub fn running_short_margin(&self) -> u64 {
        self.get_running_stats().short_margin
    }

    pub fn running_short_quantity(&self) -> u64 {
        self.get_running_stats().short_quantity
    }

    pub fn running_margin(&self) -> u64 {
        self.running_long_margin() + self.running_short_margin()
    }

    pub fn running_quantity(&self) -> u64 {
        self.running_long_quantity() + self.running_short_quantity()
    }

    pub fn running_pl(&self) -> i64 {
        self.get_running_stats().pl
    }

    pub fn running_fees(&self) -> u64 {
        self.get_running_stats().fees
    }

    /// Returns the number of closed trades
    pub fn closed_len(&self) -> usize {
        self.closed_len
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
        self.running_pl() + self.closed_pl
    }

    pub fn fees(&self) -> u64 {
        self.running_fees() + self.closed_fees
    }

    pub fn summary(&self) -> String {
        let mut result = String::new();

        result.push_str("timing:\n");
        result.push_str(&format!(
            "  last_tick_time: {}\n",
            self.last_tick_time().format_local_millis()
        ));
        let lttl_str = self
            .last_trade_time()
            .map_or("null".to_string(), |lttl| lttl.format_local_millis());
        result.push_str(&format!("  last_trade_time: {lttl_str}\n"));

        result.push_str(&format!("balance: {}\n", self.balance));
        result.push_str(&format!("market_price: {:.1}\n", self.market_price));

        result.push_str("running_positions:\n");
        result.push_str("  long:\n");
        result.push_str(&format!("    trades: {}\n", self.running_long_len()));
        result.push_str(&format!("    margin: {}\n", self.running_long_margin()));
        result.push_str(&format!("    quantity: {}\n", self.running_long_quantity()));
        result.push_str("  short:\n");
        result.push_str(&format!("    trades: {}\n", self.running_short_len()));
        result.push_str(&format!("    margin: {}\n", self.running_short_margin()));
        result.push_str(&format!(
            "    quantity: {}\n",
            self.running_short_quantity()
        ));

        result.push_str("running_metrics:\n");
        result.push_str(&format!("  pl: {}\n", self.running_pl()));
        result.push_str(&format!("  fees: {}\n", self.running_fees()));
        result.push_str(&format!("  total_margin: {}\n", self.running_margin()));

        result.push_str("closed_positions:\n");
        result.push_str(&format!("  trades: {}\n", self.closed_len));
        result.push_str(&format!("  pl: {}\n", self.closed_pl));
        result.push_str(&format!("  fees: {}", self.closed_fees));

        result
    }

    pub fn running_trades_table(&self) -> String {
        let sorted_running = self.get_running_sorted();

        if sorted_running.is_empty() {
            return "No running trades.".to_string();
        }

        let mut table = String::new();

        table.push_str(&format!(
            "{:>14} | {:>5} | {:>11} | {:>11} | {:>11} | {:>11} | {:>5} | {:>11} | {:>8} | {:>11} | {:>11} | {:>11}",
            "creation_time",
            "side",
            "quantity",
            "price",
            "liquidation",
            "stoploss",
            "TSL",
            "takeprofit",
            "leverage",
            "margin",
            "pl",
            "fees"
        ));

        table.push_str(&format!("\n{}", "-".repeat(153)));

        for (trade, tsl) in sorted_running {
            let creation_time = trade
                .creation_ts()
                .with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M");
            let stoploss_str = trade
                .stoploss()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let tsl_str = tsl.map_or("N/A".to_string(), |tsl| format!("{:.1}%", tsl.into_f64()));
            let takeprofit_str = trade
                .takeprofit()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let total_margin = trade.margin().into_i64() + trade.maintenance_margin().max(0);
            let pl = trade.est_pl(self.market_price);
            let total_fees = trade.opening_fee() + trade.closing_fee();

            table.push_str(&format!(
                "\n{:>14} | {:>5} | {:>11} | {:>11.1} | {:>11.1} | {:>11} | {:>5} | {:>11} | {:>8.2} | {:>11} | {:>11} | {:>11}",
                creation_time,
                trade.side(),
                trade.quantity(),
                trade.price(),
                trade.liquidation(),
                stoploss_str,
                tsl_str,
                takeprofit_str,
                trade.leverage(),
                total_margin,
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

pub struct ClosedTradeHistory<T: TradeClosed>(BTreeMap<DateTime<Utc>, T>);

impl<T: TradeClosed> ClosedTradeHistory<T> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn add(&mut self, trade: T) -> Result<()> {
        if !trade.closed() || trade.exit_price().is_none() || trade.closed_ts().is_none() {
            return Err(TradeError::Generic("`trade` is not closed".to_string()));
        }

        self.0.insert(trade.creation_ts(), trade);
        Ok(())
    }

    pub fn to_table(&self) -> String {
        if self.0.is_empty() {
            return "No closed trades.".to_string();
        }

        let mut table = String::new();

        table.push_str(&format!(
            "{:>14} | {:>5} | {:>11} | {:>11} | {:>11} | {:>11} | {:>14} | {:>11} | {:>11} | {:>11}",
            "creation_time",
            "side",
            "quantity",
            "margin",
            "price",
            "exit_price",
            "exit_time",
            "pl",
            "fees",
            "net_pl"
        ));

        table.push_str(&format!("\n{}", "-".repeat(137)));

        for trade in self.0.values().rev() {
            let creation_time = trade
                .creation_ts()
                .with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M");

            // Should never panic due to `new` validation
            let exit_price = trade
                .exit_price()
                .expect("`closed` trade must have `exit_price`");
            let exit_time = trade
                .closed_ts()
                .expect("`closed` trade must have `closed_ts`")
                .with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M");

            let pl = trade.pl();
            let total_fees = trade.opening_fee() + trade.closing_fee();
            let net_pl = pl - total_fees as i64;

            table.push_str(&format!(
                "\n{:>14} | {:>5} | {:>11} | {:>11} | {:>11} | {:>11} | {:>14} | {:>11} | {:>11} | {:>11}",
                creation_time,
                trade.side(),
                trade.quantity(),
                trade.margin(),
                trade.price(),
                exit_price,
                exit_time,
                pl,
                total_fees,
                net_pl
            ));
        }

        table
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stoploss {
    Fixed(Price),
    Trailing(BoundedPercentage),
}

impl Stoploss {
    pub(crate) fn evaluate(
        &self,
        tsl_step_size: BoundedPercentage,
        market_price: Price,
    ) -> Result<(Price, Option<TradeTrailingStoploss>)> {
        match self {
            Self::Fixed(price) => Ok((*price, None)),
            Self::Trailing(tsl) => {
                if tsl_step_size > *tsl {
                    return Err(TradeError::Generic(
                        "`stoploss_perc` must be gt than `tsl_step_size`".to_string(),
                    ));
                }

                let initial_stoploss_price = market_price
                    .apply_discount(*tsl)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                Ok((initial_stoploss_price, Some(TradeTrailingStoploss(*tsl))))
            }
        }
    }

    pub fn fixed(stoploss_price: Price) -> Self {
        Self::Fixed(stoploss_price)
    }

    pub fn trailing(stoploss_perc: BoundedPercentage) -> Self {
        Self::Trailing(stoploss_perc)
    }
}

impl From<Price> for Stoploss {
    fn from(value: Price) -> Self {
        Self::Fixed(value)
    }
}

impl From<BoundedPercentage> for Stoploss {
    fn from(value: BoundedPercentage) -> Self {
        Self::Trailing(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TradeTrailingStoploss(BoundedPercentage);

impl TradeTrailingStoploss {
    pub fn prev_validated(tsl: BoundedPercentage) -> Self {
        Self(tsl)
    }

    pub fn into_f64(self) -> f64 {
        self.into()
    }
}

impl From<TradeTrailingStoploss> for f64 {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0.into_f64()
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

#[async_trait]
pub trait TradeExecutor: Send + Sync {
    async fn open_long(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<(BoundedPercentage, StoplossMode)>,
        takeprofit: Option<LowerBoundedPercentage>,
    ) -> Result<()>;

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<(BoundedPercentage, StoplossMode)>,
        takeprofit: Option<BoundedPercentage>,
    ) -> Result<()>;

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()>;

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()>;

    async fn close_trade(&self, trade_id: Uuid) -> Result<()>;

    async fn close_longs(&self) -> Result<()>;

    async fn close_shorts(&self) -> Result<()>;

    async fn close_all(&self) -> Result<()>;

    async fn trading_state(&self) -> Result<TradingState>;
}

#[async_trait]
pub trait Operator: Send + Sync {
    fn set_trade_executor(
        &mut self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;

    async fn process_signal(
        &self,
        signal: &Signal,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
}

pub(crate) struct WrappedOperator(Box<dyn Operator>);

impl WrappedOperator {
    pub fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        panic::catch_unwind(AssertUnwindSafe(|| {
            self.0.set_trade_executor(trade_executor)
        }))
        .map_err(|_| TradeError::Generic(format!("`Operator::set_trade_executor` panicked")))?
        .map_err(|e| {
            TradeError::Generic(format!(
                "`Operator::set_trade_executor` error {}",
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
