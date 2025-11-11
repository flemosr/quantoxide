use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    num::NonZeroU64,
    panic::{self, AssertUnwindSafe},
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::FutureExt;
use uuid::Uuid;

use lnm_sdk::{
    error::TradeValidationError,
    models::{
        BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Margin, Price, Quantity,
        TradeSide, TradeSize, trade_util,
    },
};

use crate::{db::models::PriceHistoryEntryLOCF, signal::Signal, util::DateTimeExt};

use super::error::{TradeCoreError, TradeCoreResult, TradeExecutorResult};

impl crate::sealed::Sealed for LnmTrade {}

/// Generic trade interface used in extension traits.
///
/// Not publically exported. Public access via the sealed [`TradeRunning`] and [`TradeClosed`]
/// traits.
pub trait TradeCore: Send + Sync + fmt::Debug + 'static {
    /// Returns the unique identifier for this trade.
    fn id(&self) -> Uuid;

    /// Returns the side of the trade (Buy or Sell).
    fn side(&self) -> TradeSide;

    /// Returns the opening fee charged when the trade was created (in satoshis).
    fn opening_fee(&self) -> u64;

    /// Returns the closing fee that will be charged when the trade closes (in satoshis).
    fn closing_fee(&self) -> u64;

    /// Returns the maintenance margin requirement (in satoshis).
    fn maintenance_margin(&self) -> i64;

    /// Returns the quantity (notional value in USD) of the trade.
    fn quantity(&self) -> Quantity;

    /// Returns the margin (collateral in satoshis) allocated to the trade.
    fn margin(&self) -> Margin;

    /// Returns the leverage multiplier applied to the trade.
    fn leverage(&self) -> Leverage;

    /// Returns the trade price.
    fn price(&self) -> Price;

    /// Returns the liquidation price at which the position will be automatically closed.
    fn liquidation(&self) -> Price;

    /// Returns the stop loss price, if set.
    fn stoploss(&self) -> Option<Price>;

    /// Returns the take profit price, if set.
    fn takeprofit(&self) -> Option<Price>;

    /// Returns the price at which the trade was closed, if applicable.
    fn exit_price(&self) -> Option<Price>;

    /// Returns the timestamp when the trade was created.
    fn creation_ts(&self) -> DateTime<Utc>;

    /// Returns the timestamp when the trade was filled, if applicable.
    fn market_filled_ts(&self) -> Option<DateTime<Utc>>;

    /// Returns the timestamp when the trade was closed, if applicable.
    fn closed_ts(&self) -> Option<DateTime<Utc>>;

    /// Returns `true` if the trade has been closed.
    fn closed(&self) -> bool;
}

impl TradeCore for LnmTrade {
    fn id(&self) -> Uuid {
        self.id()
    }

    fn side(&self) -> TradeSide {
        self.side()
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee()
    }

    fn closing_fee(&self) -> u64 {
        self.closing_fee()
    }

    fn maintenance_margin(&self) -> i64 {
        self.maintenance_margin()
    }

    fn quantity(&self) -> Quantity {
        self.quantity()
    }

    fn margin(&self) -> Margin {
        self.margin()
    }

    fn leverage(&self) -> Leverage {
        self.leverage()
    }

    fn price(&self) -> Price {
        self.price()
    }

    fn liquidation(&self) -> Price {
        self.liquidation()
    }

    fn stoploss(&self) -> Option<Price> {
        self.stoploss()
    }

    fn takeprofit(&self) -> Option<Price> {
        self.takeprofit()
    }

    fn exit_price(&self) -> Option<Price> {
        self.exit_price()
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts()
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts()
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts()
    }

    fn closed(&self) -> bool {
        self.closed()
    }
}

/// Extension trait for running trades with profit/loss and margin calculations.
///
/// Provides methods for estimating profit/loss and calculating margin adjustments for trades that
/// are currently active (running). This trait extends the [`Trade`] trait with functionality
/// specific to active positions.
///
/// This trait is sealed and not meant to be implemented outside of `quantoxide`.
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{
///     LnmTrade, TradeExecution, TradeSide, TradeSize, Leverage, Margin, Price
/// };
/// use quantoxide::trade::TradeRunning;
///
/// let trade: LnmTrade = api
///     .rest
///     .futures
///     .create_new_trade(
///         TradeSide::Buy,
///         TradeSize::from(Margin::try_from(10_000).unwrap()),
///         Leverage::try_from(10.0).unwrap(),
///         TradeExecution::Market,
///         None,
///         None,
///     )
///     .await?;
///
/// let market_price = Price::try_from(101_000.0).unwrap();
/// let estimated_pl = trade.est_pl(market_price);
/// let max_cash_in = trade.est_max_cash_in(market_price);
///
/// println!("Estimated P/L: {} sats", estimated_pl);
/// println!("Max cash-in: {} sats", max_cash_in);
/// # Ok(())
/// # }
/// ```
pub trait TradeRunning: crate::sealed::Sealed + TradeCore {
    /// Estimates the profit/loss for the trade at a given market price.
    ///
    /// Returns the estimated profit or loss in satoshis if the trade were closed at the specified
    /// market price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use lnm_sdk::models::Price;
    /// use quantoxide::trade::TradeRunning;
    ///
    /// let market_price = Price::try_from(101_000.0).unwrap();
    /// let pl = trade.est_pl(market_price);
    ///
    /// if pl > 0.0 {
    ///     println!("Profit: {} sats", pl);
    /// } else {
    ///     println!("Loss: {} sats", pl.abs());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn est_pl(&self, market_price: Price) -> f64;

    /// Estimates the maximum additional margin that can be added to the trade.
    ///
    /// Returns the maximum amount of margin (in satoshis) that can be added to reduce leverage to
    /// the minimum level (1x). Returns 0 if the trade is already at minimum leverage.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use quantoxide::trade::TradeRunning;
    ///
    /// let max_additional = trade.est_max_additional_margin();
    ///
    /// println!("Can add up to {} sats margin", max_additional);
    /// # Ok(())
    /// # }
    /// ```
    fn est_max_additional_margin(&self) -> u64 {
        if self.leverage() == Leverage::MIN {
            return 0;
        }

        let max_margin = Margin::calculate(self.quantity(), self.price(), Leverage::MIN);

        let max_add_margin = max_margin
            .into_u64()
            .saturating_sub(self.margin().into_u64());

        return max_add_margin;
    }

    /// Estimates the maximum margin that can be withdrawn from the trade.
    ///
    /// Returns the maximum amount of margin (in satoshis) that can be withdrawn while maintaining
    /// the position at maximum leverage. Includes any extractable profit.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use lnm_sdk::models::Price;
    /// use quantoxide::trade::TradeRunning;
    ///
    /// let market_price = Price::try_from(101_000.0).unwrap();
    /// let max_withdrawal = trade.est_max_cash_in(market_price);
    ///
    /// println!("Can withdraw up to {} sats", max_withdrawal);
    /// # Ok(())
    /// # }
    /// ```
    fn est_max_cash_in(&self, market_price: Price) -> u64 {
        let extractable_pl = self.est_pl(market_price).max(0.) as u64;

        let min_margin = Margin::calculate(self.quantity(), self.price(), Leverage::MAX);

        let max_cash_in = self
            .margin()
            .into_u64()
            .saturating_sub(min_margin.into_u64())
            + extractable_pl;

        return max_cash_in;
    }

    /// Calculates the collateral adjustment needed to achieve a target liquidation price.
    ///
    /// Returns a positive value if margin needs to be added, or a negative value if margin can be
    /// withdrawn to reach the target liquidation price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>>  {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use lnm_sdk::models::Price;
    /// use quantoxide::trade::TradeRunning;
    ///
    /// let target_liquidation = Price::try_from(95_000.0).unwrap();
    /// let market_price = Price::try_from(100_000.0).unwrap();
    ///
    /// let delta = trade.est_collateral_delta_for_liquidation(
    ///     target_liquidation,
    ///     market_price
    /// )?;
    ///
    /// if delta > 0 {
    ///     println!("Add {} sats to reach target liquidation", delta);
    /// } else {
    ///     println!("Remove {} sats to reach target liquidation", delta.abs());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn est_collateral_delta_for_liquidation(
        &self,
        target_liquidation: Price,
        market_price: Price,
    ) -> Result<i64, TradeValidationError> {
        trade_util::evaluate_collateral_delta_for_liquidation(
            self.side(),
            self.quantity(),
            self.margin(),
            self.price(),
            self.liquidation(),
            target_liquidation,
            market_price,
        )
    }
}

impl TradeRunning for LnmTrade {
    fn est_pl(&self, market_price: Price) -> f64 {
        trade_util::estimate_pl(self.side(), self.quantity(), self.price(), market_price)
    }
}

/// Extension trait for closed trades.
///
/// Provides access to the final profit/loss of a trade that has been closed. This trait extends the
/// [`Trade`] trait with functionality specific to completed positions.
///
/// This trait is sealed and not meant to be implemented outside of `quantoxide`.
pub trait TradeClosed: crate::sealed::Sealed + TradeCore {
    /// Returns the realized profit/loss of the closed trade in satoshis.
    ///
    /// A positive value indicates profit, while a negative value indicates a loss.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(closed_trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// use quantoxide::trade::TradeClosed;
    ///
    /// let pl = closed_trade.pl();
    ///
    /// println!("Realized P/L: {} sats", pl);
    /// # Ok(())
    /// # }
    /// ```
    fn pl(&self) -> i64;
}

impl TradeClosed for LnmTrade {
    fn pl(&self) -> i64 {
        self.pl()
    }
}

#[derive(Debug)]
pub struct RunningTradesMap<T: TradeRunning + ?Sized> {
    trades: BTreeMap<(DateTime<Utc>, Uuid), (Arc<T>, Option<TradeTrailingStoploss>)>,
    id_to_time: HashMap<Uuid, DateTime<Utc>>,
}

pub type DynRunningTradesMap = RunningTradesMap<dyn TradeRunning>;

impl<T: TradeRunning + ?Sized> RunningTradesMap<T> {
    pub(super) fn new() -> Self {
        Self {
            trades: BTreeMap::new(),
            id_to_time: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    pub(super) fn add(&mut self, trade: Arc<T>, trade_tsl: Option<TradeTrailingStoploss>) {
        self.id_to_time.insert(trade.id(), trade.creation_ts());
        self.trades
            .insert((trade.creation_ts(), trade.id()), (trade, trade_tsl));
    }

    pub fn len(&self) -> usize {
        self.id_to_time.len()
    }

    pub fn contains(&self, trade_id: &Uuid) -> bool {
        self.id_to_time.get(trade_id).is_some()
    }

    pub fn get_trade_by_id(&self, id: Uuid) -> Option<&(Arc<T>, Option<TradeTrailingStoploss>)> {
        self.id_to_time
            .get(&id)
            .and_then(|creation_ts| self.trades.get(&(*creation_ts, id)))
    }

    pub(super) fn get_trade_by_id_mut(
        &mut self,
        id: Uuid,
    ) -> Option<&mut (Arc<T>, Option<TradeTrailingStoploss>)> {
        self.id_to_time
            .get(&id)
            .and_then(|creation_ts| self.trades.get_mut(&(*creation_ts, id)))
    }

    pub fn trades_desc(&self) -> impl Iterator<Item = &(Arc<T>, Option<TradeTrailingStoploss>)> {
        self.trades.iter().rev().map(|(_, trade_tuple)| trade_tuple)
    }

    pub(super) fn trades_desc_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut (Arc<T>, Option<TradeTrailingStoploss>)> {
        self.trades
            .iter_mut()
            .rev()
            .map(|(_, trade_tuple)| trade_tuple)
    }
}

impl<T: TradeRunning> RunningTradesMap<T> {
    pub(super) fn into_dyn(self) -> DynRunningTradesMap {
        let dyn_trades = self
            .trades
            .into_iter()
            .map(|(key, (trade, stoploss))| {
                let dyn_trade: Arc<dyn TradeRunning> = trade;
                (key, (dyn_trade, stoploss))
            })
            .collect();

        RunningTradesMap {
            trades: dyn_trades,
            id_to_time: self.id_to_time,
        }
    }
}

impl<T: TradeRunning + ?Sized> Clone for RunningTradesMap<T> {
    fn clone(&self) -> Self {
        Self {
            trades: self.trades.clone(),
            id_to_time: self.id_to_time.clone(),
        }
    }
}

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
    running_map: DynRunningTradesMap,
    running_stats: OnceLock<RunningStats>,
    closed_len: usize,
    closed_pl: i64,
    closed_fees: u64,
}

impl TradingState {
    pub(super) fn new(
        last_tick_time: DateTime<Utc>,
        balance: u64,
        market_price: Price,
        last_trade_time: Option<DateTime<Utc>>,
        running_map: DynRunningTradesMap,
        closed_len: usize,
        closed_pl: i64,
        closed_fees: u64,
    ) -> Self {
        Self {
            last_tick_time,
            balance,
            market_price,
            last_trade_time,
            running_map,
            running_stats: OnceLock::new(),
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

            for (trade, _) in self.running_map.trades_desc() {
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
                pl += trade.est_pl(self.market_price).floor() as i64;
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

    pub fn last_tick_time(&self) -> DateTime<Utc> {
        self.last_tick_time
    }

    pub fn total_net_value(&self) -> u64 {
        self.balance
            .saturating_add(self.running_margin())
            .saturating_add_signed(self.running_pl())
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

    pub fn running_map(&self) -> &DynRunningTradesMap {
        &self.running_map
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

        result.push_str(&format!("total_net_value: {}\n", self.total_net_value()));
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
        if self.running_map.is_empty() {
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

        for (trade, tsl) in self.running_map.trades_desc() {
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
            let pl = trade.est_pl(self.market_price).floor() as i64;
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

pub struct ClosedTradeHistory<T: TradeClosed>(BTreeMap<(DateTime<Utc>, Uuid), T>);

impl<T: TradeClosed> ClosedTradeHistory<T> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn add(&mut self, trade: T) -> TradeCoreResult<()> {
        if !trade.closed() || trade.exit_price().is_none() || trade.closed_ts().is_none() {
            return Err(TradeCoreError::TradeNotClosed {
                trade_id: trade.id(),
            });
        }

        self.0.insert((trade.creation_ts(), trade.id()), trade);
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
    pub(super) fn evaluate(
        &self,
        tsl_step_size: BoundedPercentage,
        side: TradeSide,
        market_price: Price,
    ) -> TradeCoreResult<(Price, Option<TradeTrailingStoploss>)> {
        match self {
            Self::Fixed(price) => Ok((*price, None)),
            Self::Trailing(tsl) => {
                if tsl_step_size > *tsl {
                    return Err(TradeCoreError::InvalidStoplossSmallerThanTrailingStepSize {
                        tsl: *tsl,
                        tsl_step_size,
                    });
                }

                let initial_stoploss_price = match side {
                    TradeSide::Buy => market_price.apply_discount(*tsl).map_err(|e| {
                        TradeCoreError::InvalidPriceApplyDiscount {
                            price: market_price,
                            discount: *tsl,
                            e,
                        }
                    })?,
                    TradeSide::Sell => market_price.apply_gain((*tsl).into()).map_err(|e| {
                        TradeCoreError::InvalidPriceApplyGain {
                            price: market_price,
                            gain: (*tsl).into(),
                            e,
                        }
                    })?,
                };

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
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> TradeExecutorResult<()>;

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> TradeExecutorResult<()>;

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()>;

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()>;

    async fn close_trade(&self, trade_id: Uuid) -> TradeExecutorResult<()>;

    async fn close_longs(&self) -> TradeExecutorResult<()>;

    async fn close_shorts(&self) -> TradeExecutorResult<()>;

    async fn close_all(&self) -> TradeExecutorResult<()>;

    async fn trading_state(&self) -> TradeExecutorResult<TradingState>;
}

#[async_trait]
pub trait SignalOperator: Send + Sync {
    fn set_trade_executor(
        &mut self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;

    async fn process_signal(
        &self,
        signal: &Signal,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
}

pub(super) struct WrappedSignalOperator(Box<dyn SignalOperator>);

impl WrappedSignalOperator {
    pub fn set_trade_executor(
        &mut self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> TradeCoreResult<()> {
        panic::catch_unwind(AssertUnwindSafe(|| {
            self.0.set_trade_executor(trade_executor)
        }))
        .map_err(|e| TradeCoreError::SignalOperatorSetTradeExecutorPanicked(e.into()))?
        .map_err(|e| TradeCoreError::SignalOperatorSetTradeExecutorError(e.to_string()))
    }

    pub async fn process_signal(&self, signal: &Signal) -> TradeCoreResult<()> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.process_signal(signal)))
            .await
            .map_err(|e| TradeCoreError::SignalOperatorProcessSignalPanicked(e.into()))?
            .map_err(|e| TradeCoreError::SignalOperatorProcessSignalError(e.to_string()))
    }
}

impl From<Box<dyn SignalOperator>> for WrappedSignalOperator {
    fn from(value: Box<dyn SignalOperator>) -> Self {
        Self(value)
    }
}

#[async_trait]
pub trait RawOperator: Send + Sync {
    fn set_trade_executor(
        &mut self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;

    fn iteration_interval_secs(&self) -> usize;

    fn context_window_secs(&self) -> usize;

    async fn iterate(
        &self,
        context: &[PriceHistoryEntryLOCF],
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
}

pub(super) struct WrappedRawOperator(Box<dyn RawOperator>);

impl WrappedRawOperator {
    pub fn set_trade_executor(
        &mut self,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> TradeCoreResult<()> {
        panic::catch_unwind(AssertUnwindSafe(|| {
            self.0.set_trade_executor(trade_executor)
        }))
        .map_err(|e| TradeCoreError::RawOperatorSetTradeExecutorPanicked(e.into()))?
        .map_err(|e| TradeCoreError::RawOperatorSetTradeExecutorError(e.to_string()))
    }

    pub fn iteration_interval(&self) -> TradeCoreResult<Duration> {
        let interval_secs =
            panic::catch_unwind(AssertUnwindSafe(|| self.0.iteration_interval_secs()))
                .map_err(|e| TradeCoreError::RawOperatorIterationIntervalPanicked(e.into()))?;
        Ok(Duration::seconds(interval_secs as i64))
    }

    pub fn context_window_secs(&self) -> TradeCoreResult<usize> {
        let window = panic::catch_unwind(AssertUnwindSafe(|| self.0.context_window_secs()))
            .map_err(|e| TradeCoreError::RawOperatorContextWindowPanicked(e.into()))?;
        Ok(window)
    }

    pub async fn iterate(&self, entries: &[PriceHistoryEntryLOCF]) -> TradeCoreResult<()> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.iterate(entries)))
            .await
            .map_err(|e| TradeCoreError::RawOperatorIteratePanicked(e.into()))?
            .map_err(|e| TradeCoreError::RawOperatorIterateError(e.to_string()))
    }
}

impl From<Box<dyn RawOperator>> for WrappedRawOperator {
    fn from(value: Box<dyn RawOperator>) -> Self {
        Self(value)
    }
}

pub(super) trait TradeRunningExt: TradeRunning {
    fn next_stoploss_update_trigger(
        &self,
        tsl_step_size: BoundedPercentage,
        trade_tsl: TradeTrailingStoploss,
    ) -> TradeCoreResult<Price> {
        let tsl = trade_tsl.into();
        if tsl_step_size > tsl {
            return Err(TradeCoreError::InvalidStoplossSmallerThanTrailingStepSize {
                tsl,
                tsl_step_size,
            });
        }

        let curr_stoploss =
            self.stoploss()
                .ok_or_else(|| TradeCoreError::NoNextTriggerTradeStoplossNotSet {
                    trade_id: self.id(),
                })?;

        let price_trigger = match self.side() {
            TradeSide::Buy => {
                let next_stoploss =
                    curr_stoploss
                        .apply_gain(tsl_step_size.into())
                        .map_err(|e| TradeCoreError::InvalidPriceApplyGain {
                            price: curr_stoploss,
                            gain: tsl_step_size.into(),
                            e,
                        })?;
                next_stoploss.apply_gain(trade_tsl.into()).map_err(|e| {
                    TradeCoreError::InvalidPriceApplyGain {
                        price: next_stoploss,
                        gain: trade_tsl.into(),
                        e,
                    }
                })?
            }
            TradeSide::Sell => {
                let next_stoploss = curr_stoploss.apply_discount(tsl_step_size).map_err(|e| {
                    TradeCoreError::InvalidPriceApplyDiscount {
                        price: curr_stoploss,
                        discount: tsl_step_size.into(),
                        e,
                    }
                })?;
                next_stoploss
                    .apply_discount(trade_tsl.into())
                    .map_err(|e| TradeCoreError::InvalidPriceApplyDiscount {
                        price: next_stoploss,
                        discount: trade_tsl.into(),
                        e,
                    })?
            }
        };

        Ok(price_trigger)
    }

    fn eval_trigger_bounds(
        &self,
        tsl_step_size: BoundedPercentage,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> TradeCoreResult<(Price, Price)> {
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
    ) -> TradeCoreResult<Option<Price>> {
        let next_stoploss_update_trigger = self
            .next_stoploss_update_trigger(tsl_step_size, trade_tsl)?
            .into_f64();

        let new_stoploss = match self.side() {
            TradeSide::Buy => {
                if range_max >= next_stoploss_update_trigger {
                    let new_stoploss = Price::round(range_max).map_err(|e| {
                        TradeCoreError::InvalidPriceRounding {
                            price: range_max,
                            e,
                        }
                    })?;
                    let new_stoploss =
                        new_stoploss.apply_discount(trade_tsl.into()).map_err(|e| {
                            TradeCoreError::InvalidPriceApplyDiscount {
                                price: new_stoploss,
                                discount: trade_tsl.into(),
                                e,
                            }
                        })?;

                    Some(new_stoploss)
                } else {
                    None
                }
            }
            TradeSide::Sell => {
                if range_min <= next_stoploss_update_trigger {
                    let new_stoploss = Price::round(range_min).map_err(|e| {
                        TradeCoreError::InvalidPriceRounding {
                            price: range_min,
                            e,
                        }
                    })?;
                    let new_stoploss = new_stoploss.apply_gain(trade_tsl.into()).map_err(|e| {
                        TradeCoreError::InvalidPriceApplyGain {
                            price: new_stoploss,
                            gain: trade_tsl.into(),
                            e,
                        }
                    })?;

                    Some(new_stoploss)
                } else {
                    None
                }
            }
        };

        Ok(new_stoploss)
    }
}

// Implement `TradeRunningExt` for any type that implements `TradeRunning`
impl<T: TradeRunning + ?Sized> TradeRunningExt for T {}

#[derive(Debug, Clone)]
pub(super) enum PriceTrigger {
    NotSet,
    Set { min: Price, max: Price },
}

impl PriceTrigger {
    pub fn new() -> Self {
        Self::NotSet
    }

    pub fn update<T: TradeRunningExt + ?Sized>(
        &mut self,
        tsl_step_size: BoundedPercentage,
        trade: &T,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> TradeCoreResult<()> {
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
