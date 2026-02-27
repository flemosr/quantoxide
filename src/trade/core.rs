use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    num::NonZeroU64,
    panic::{self, AssertUnwindSafe},
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;
use uuid::Uuid;

use lnm_sdk::api_v3::{
    error::TradeValidationError,
    models::{
        ClientId, Leverage, Margin, Percentage, PercentageCapped, Price, Quantity, Trade,
        TradeSide, TradeSize, trade_util,
    },
};

use crate::{
    db::models::OhlcCandleRow,
    error::Result as GeneralResult,
    shared::{Lookback, MinIterationInterval},
    signal::Signal,
    util::DateTimeExt,
};

use super::error::{TradeCoreError, TradeCoreResult, TradeExecutorResult};

impl crate::sealed::Sealed for Trade {}

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
    fn created_at(&self) -> DateTime<Utc>;

    /// Returns the timestamp when the trade was filled, if applicable.
    fn filled_at(&self) -> Option<DateTime<Utc>>;

    /// Returns the timestamp when the trade was closed, if applicable.
    fn closed_at(&self) -> Option<DateTime<Utc>>;

    /// Returns `true` if the trade has been closed.
    fn closed(&self) -> bool;

    /// Returns the client-provided identifier for this trade, if set.
    fn client_id(&self) -> Option<&ClientId>;
}

impl TradeCore for Trade {
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

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at()
    }

    fn filled_at(&self) -> Option<DateTime<Utc>> {
        self.filled_at()
    }

    fn closed_at(&self) -> Option<DateTime<Utc>> {
        self.closed_at()
    }

    fn closed(&self) -> bool {
        self.closed()
    }

    fn client_id(&self) -> Option<&ClientId> {
        self.client_id()
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
/// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use quantoxide::{
///     models::{Trade, TradeExecution, TradeSide, TradeSize, Leverage, Margin, Price},
///     trade::TradeRunning,
/// };
///
/// let trade: Trade = rest
///     .futures_isolated
///     .new_trade(
///         TradeSide::Buy,
///         TradeSize::from(Margin::try_from(10_000).unwrap()),
///         Leverage::try_from(10.0).unwrap(),
///         TradeExecution::Market,
///         None,
///         None,
///         None
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
    /// # fn example(trade: quantoxide::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use quantoxide::{models::Price, trade::TradeRunning};
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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

        max_margin.as_u64().saturating_sub(self.margin().as_u64())
    }

    /// Estimates the maximum margin that can be withdrawn from the trade.
    ///
    /// Returns the maximum amount of margin (in satoshis) that can be withdrawn while maintaining
    /// the position at maximum leverage. Includes any extractable profit.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: quantoxide::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use quantoxide::{models::Price, trade::TradeRunning};
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

        let excess_margin = self.margin().as_u64().saturating_sub(min_margin.as_u64());

        excess_margin + extractable_pl
    }

    /// Calculates the collateral adjustment needed to achieve a target liquidation price.
    ///
    /// Returns a positive value if margin needs to be added, or a negative value if margin can be
    /// withdrawn to reach the target liquidation price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: quantoxide::models::Trade) -> Result<(), Box<dyn std::error::Error>>  {
    /// // Assuming `trade` impl `TradeRunning`
    ///
    /// use quantoxide::{models::Price, trade::TradeRunning};
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

impl TradeRunning for Trade {
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
    /// # async fn example(closed_trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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

impl TradeClosed for Trade {
    fn pl(&self) -> i64 {
        self.pl()
    }
}

/// A reference to a trade, containing `(creation_timestamp, trade_uuid)`.
pub type TradeReference = (DateTime<Utc>, Uuid);

/// A collection of running trades indexed by creation time and UUID, with optional trailing
/// stoploss metadata. Provides efficient lookups by trade ID and chronological iteration.
#[derive(Debug)]
pub struct RunningTradesMap<T: TradeRunning + ?Sized> {
    trades: BTreeMap<TradeReference, (Arc<T>, Option<TradeTrailingStoploss>)>,
    id_to_time: HashMap<Uuid, DateTime<Utc>>,
}

/// Type alias for a dynamically dispatched running trades map.
pub type DynRunningTradesMap = RunningTradesMap<dyn TradeRunning>;

impl<T: TradeRunning + ?Sized> RunningTradesMap<T> {
    pub(super) fn new() -> Self {
        Self {
            trades: BTreeMap::new(),
            id_to_time: HashMap::new(),
        }
    }

    /// Returns `true` if the map contains no trades.
    pub fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    pub(super) fn add(&mut self, trade: Arc<T>, trade_tsl: Option<TradeTrailingStoploss>) {
        self.id_to_time.insert(trade.id(), trade.created_at());
        self.trades
            .insert((trade.created_at(), trade.id()), (trade, trade_tsl));
    }

    /// Returns the number of trades in the map.
    pub fn len(&self) -> usize {
        self.id_to_time.len()
    }

    /// Returns `true` if the map contains a trade with the specified ID.
    pub fn contains(&self, trade_id: &Uuid) -> bool {
        self.id_to_time.contains_key(trade_id)
    }

    /// Returns a reference to the trade and its trailing stoploss metadata for the given trade ID.
    pub fn get_by_id(&self, id: Uuid) -> Option<&(Arc<T>, Option<TradeTrailingStoploss>)> {
        self.id_to_time
            .get(&id)
            .and_then(|creation_ts| self.trades.get(&(*creation_ts, id)))
    }

    pub(super) fn get_by_id_mut(
        &mut self,
        id: Uuid,
    ) -> Option<&mut (Arc<T>, Option<TradeTrailingStoploss>)> {
        self.id_to_time
            .get(&id)
            .and_then(|creation_ts| self.trades.get_mut(&(*creation_ts, id)))
    }

    /// Returns an iterator over trade references and their data in ascending chronological order
    /// (oldest first).
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&TradeReference, &(Arc<T>, Option<TradeTrailingStoploss>))> {
        self.trades.iter()
    }

    /// Returns an iterator over trade references in ascending chronological order (oldest first).
    pub fn keys(&self) -> impl Iterator<Item = &TradeReference> {
        self.trades.keys()
    }

    /// Returns an iterator over trades and their trailing stoploss metadata in ascending
    /// chronological order (oldest first).
    pub fn values(&self) -> impl Iterator<Item = &(Arc<T>, Option<TradeTrailingStoploss>)> {
        self.trades.values()
    }

    /// Returns an iterator over trades in descending chronological order (newest first).
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

impl<'a, T: TradeRunning + ?Sized> IntoIterator for &'a RunningTradesMap<T> {
    type Item = (
        &'a TradeReference,
        &'a (Arc<T>, Option<TradeTrailingStoploss>),
    );
    type IntoIter = std::collections::btree_map::Iter<
        'a,
        TradeReference,
        (Arc<T>, Option<TradeTrailingStoploss>),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.trades.iter()
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

/// Comprehensive snapshot of the current trading state including balance, running trades, and
/// performance metrics. This type provides a complete view of a trading session at a specific point
/// in time.
#[derive(Debug, Clone)]
pub struct TradingState {
    last_tick_time: DateTime<Utc>,
    balance: u64,
    market_price: Price,
    last_trade_time: Option<DateTime<Utc>>,
    running_map: DynRunningTradesMap,
    running_stats: OnceLock<RunningStats>,
    funding_fees: i64,
    realized_pl: i64,
    closed_history: Arc<ClosedTradeHistory>,
    closed_fees: u64,
}

impl TradingState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        last_tick_time: DateTime<Utc>,
        balance: u64,
        market_price: Price,
        last_trade_time: Option<DateTime<Utc>>,
        running_map: DynRunningTradesMap,
        funding_fees: i64,
        realized_pl: i64,
        closed_history: Arc<ClosedTradeHistory>,
        closed_fees: u64,
    ) -> Self {
        Self {
            last_tick_time,
            balance,
            market_price,
            last_trade_time,
            running_map,
            running_stats: OnceLock::new(),
            funding_fees,
            realized_pl,
            closed_history,
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
                            trade.margin().as_u64() + trade.maintenance_margin().max(0) as u64;
                        long_quantity += trade.quantity().as_u64();
                    }
                    TradeSide::Sell => {
                        short_len += 1;
                        short_margin +=
                            trade.margin().as_u64() + trade.maintenance_margin().max(0) as u64;
                        short_quantity += trade.quantity().as_u64();
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

    /// Returns the timestamp of the last market price update.
    pub fn last_tick_time(&self) -> DateTime<Utc> {
        self.last_tick_time
    }

    /// Returns the total net value including balance, locked margin, and unrealized profit/loss.
    pub fn total_net_value(&self) -> u64 {
        self.balance
            .saturating_add(self.running_margin())
            .saturating_add_signed(self.running_pl())
    }

    /// Returns the available balance (in satoshis) not locked in trades.
    pub fn balance(&self) -> u64 {
        self.balance
    }

    /// Returns the current market price used for calculating unrealized profit/loss.
    pub fn market_price(&self) -> Price {
        self.market_price
    }

    /// Returns the timestamp of the most recent trade action, if any.
    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.last_trade_time
    }

    /// Returns a reference to the map of currently running trades.
    pub fn running_map(&self) -> &DynRunningTradesMap {
        &self.running_map
    }

    /// Returns the number of running long positions.
    pub fn running_long_len(&self) -> usize {
        self.get_running_stats().long_len
    }

    /// Returns the total locked margin for long positions (in satoshis).
    pub fn running_long_margin(&self) -> u64 {
        self.get_running_stats().long_margin
    }

    /// Returns the total notional quantity for long positions (in USD).
    pub fn running_long_quantity(&self) -> u64 {
        self.get_running_stats().long_quantity
    }

    /// Returns the number of running short positions.
    pub fn running_short_len(&self) -> usize {
        self.get_running_stats().short_len
    }

    /// Returns the total locked margin for short positions (in satoshis).
    pub fn running_short_margin(&self) -> u64 {
        self.get_running_stats().short_margin
    }

    /// Returns the total notional quantity for short positions (in USD).
    pub fn running_short_quantity(&self) -> u64 {
        self.get_running_stats().short_quantity
    }

    /// Returns the total locked margin across all running positions (in satoshis).
    pub fn running_margin(&self) -> u64 {
        self.running_long_margin() + self.running_short_margin()
    }

    /// Returns the total notional quantity across all running positions (in USD).
    pub fn running_quantity(&self) -> u64 {
        self.running_long_quantity() + self.running_short_quantity()
    }

    /// Returns the total unrealized profit/loss across all running positions (in satoshis).
    pub fn running_pl(&self) -> i64 {
        self.get_running_stats().pl
    }

    /// Returns the total fees for all running positions (in satoshis).
    pub fn running_fees(&self) -> u64 {
        self.get_running_stats().fees
    }

    /// Returns the net funding fees across all settlements (in satoshis).
    ///
    /// Positive -> net cost
    /// Negative -> net revenue
    pub fn funding_fees(&self) -> i64 {
        self.funding_fees
    }

    /// Returns the total realized profit/loss including closed trades and cashed-in profits from
    /// running trades (in satoshis).
    pub fn realized_pl(&self) -> i64 {
        self.realized_pl
    }

    /// Returns a reference to the closed trade history.
    pub fn closed_history(&self) -> &Arc<ClosedTradeHistory> {
        &self.closed_history
    }

    /// Returns the number of closed trades.
    pub fn closed_len(&self) -> usize {
        self.closed_history.len()
    }

    /// Returns the total fees paid for closed trades (in satoshis).
    pub fn closed_fees(&self) -> u64 {
        self.closed_fees
    }

    /// Returns the net profit/loss of closed trades after fees (in satoshis).
    pub fn closed_net_pl(&self) -> i64 {
        self.realized_pl - self.closed_fees() as i64
    }

    /// Returns the total profit/loss combining both running and realized P/L (in satoshis).
    pub fn pl(&self) -> i64 {
        self.running_pl() + self.realized_pl
    }

    /// Returns the total fees across both running and closed trades (in satoshis).
    pub fn fees(&self) -> u64 {
        self.running_fees() + self.closed_fees()
    }

    /// Returns a formatted string containing a comprehensive summary of the trading state including
    /// timing information, balances, positions, and metrics.
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
        result.push_str(&format!("funding_fees: {}\n", self.funding_fees));
        result.push_str(&format!("realized_pl: {}\n", self.realized_pl));
        result.push_str("closed_positions:\n");
        result.push_str(&format!("  trades: {}\n", self.closed_len()));
        result.push_str(&format!("  fees: {}", self.closed_fees));

        result
    }

    /// Returns a formatted table displaying all running trades with their details including price,
    /// leverage, margin, profit/loss, and fees.
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
                .created_at()
                .with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M");
            let stoploss_str = trade
                .stoploss()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let tsl_str = tsl.map_or("N/A".to_string(), |tsl| format!("{:.1}%", tsl.as_f64()));
            let takeprofit_str = trade
                .takeprofit()
                .map_or("N/A".to_string(), |sl| format!("{:.1}", sl));
            let total_margin = trade.margin().as_i64() + trade.maintenance_margin().max(0);
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

/// A chronologically ordered collection of closed trades. Stores completed trades indexed by
/// creation time and UUID. Uses dynamic dispatch to support heterogeneous trade types.
pub struct ClosedTradeHistory {
    trades: BTreeMap<(DateTime<Utc>, Uuid), Arc<dyn TradeClosed>>,
    /// Maps UUID to creation timestamp for O(1) lookups by trade ID.
    id_to_time: HashMap<Uuid, DateTime<Utc>>,
}

impl ClosedTradeHistory {
    /// Creates a new empty closed trade history.
    pub fn new() -> Self {
        Self {
            trades: BTreeMap::new(),
            id_to_time: HashMap::new(),
        }
    }

    /// Adds a closed trade (as Arc) to the history. Returns an error if the trade is not properly
    /// closed.
    pub(super) fn add(&mut self, trade: Arc<dyn TradeClosed>) -> TradeCoreResult<()> {
        if !trade.closed() || trade.exit_price().is_none() || trade.closed_at().is_none() {
            return Err(TradeCoreError::TradeNotClosed {
                trade_id: trade.id(),
            });
        }

        let id = trade.id();
        let created_at = trade.created_at();
        self.trades.insert((created_at, id), trade);
        self.id_to_time.insert(id, created_at);
        Ok(())
    }

    /// Returns a reference to the trade with the given UUID, if it exists.
    pub fn get_by_id(&self, id: Uuid) -> Option<&Arc<dyn TradeClosed>> {
        let creation_ts = self.id_to_time.get(&id)?;
        self.trades.get(&(*creation_ts, id))
    }

    /// Returns `true` if the history contains no trades.
    pub fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    /// Returns the number of closed trades in the history.
    pub fn len(&self) -> usize {
        self.trades.len()
    }

    /// Returns an iterator over trades in ascending chronological order (oldest first).
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn TradeClosed>> {
        self.trades.values()
    }

    /// Returns an iterator over trades in descending chronological order (newest first).
    pub fn iter_desc(&self) -> impl Iterator<Item = &Arc<dyn TradeClosed>> {
        self.trades.values().rev()
    }

    /// Returns a formatted table displaying all closed trades with their entry/exit details,
    /// profit/loss, and fees.
    pub fn to_table(&self) -> String {
        if self.trades.is_empty() {
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

        for trade in self.trades.values().rev() {
            let creation_time = trade
                .created_at()
                .with_timezone(&chrono::Local)
                .format("%y-%m-%d %H:%M");

            // Should never panic due to `add` validation
            let exit_price = trade
                .exit_price()
                .expect("`closed` trade must have `exit_price`");
            let exit_time = trade
                .closed_at()
                .expect("`closed` trade must have `closed_at`")
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

impl Clone for ClosedTradeHistory {
    fn clone(&self) -> Self {
        Self {
            trades: self.trades.clone(),
            id_to_time: self.id_to_time.clone(),
        }
    }
}

impl Default for ClosedTradeHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ClosedTradeHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClosedTradeHistory")
            .field("len", &self.trades.len())
            .finish()
    }
}

/// Stoploss configuration specifying either a fixed price level or a trailing percentage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stoploss {
    /// Fixed stoploss at a specific price.
    Fixed(Price),
    /// Trailing stoploss that follows the market price by a percentage.
    Trailing(PercentageCapped),
}

impl Stoploss {
    pub(super) fn evaluate(
        &self,
        tsl_step_size: PercentageCapped,
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

    /// Creates a fixed stoploss at the specified price.
    pub fn fixed(stoploss_price: Price) -> Self {
        Self::Fixed(stoploss_price)
    }

    /// Creates a trailing stoploss with the specified percentage.
    pub fn trailing(stoploss_perc: PercentageCapped) -> Self {
        Self::Trailing(stoploss_perc)
    }
}

impl From<Price> for Stoploss {
    fn from(value: Price) -> Self {
        Self::Fixed(value)
    }
}

impl From<PercentageCapped> for Stoploss {
    fn from(value: PercentageCapped) -> Self {
        Self::Trailing(value)
    }
}

/// Metadata for a trailing stoploss that has been validated and applied to a trade. Wraps a
/// percentage value that determines how the stoploss follows market price movements.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TradeTrailingStoploss(PercentageCapped);

impl TradeTrailingStoploss {
    pub(crate) fn prev_validated(tsl: PercentageCapped) -> Self {
        Self(tsl)
    }

    /// Returns the trailing stoploss percentage as an f64 value.
    pub fn as_f64(self) -> f64 {
        self.0.as_f64()
    }
}

impl From<TradeTrailingStoploss> for f64 {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0.as_f64()
    }
}

impl From<TradeTrailingStoploss> for PercentageCapped {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0
    }
}

impl From<TradeTrailingStoploss> for Percentage {
    fn from(value: TradeTrailingStoploss) -> Self {
        value.0.into()
    }
}

/// Trait for executing trading operations including opening/closing positions and managing margin.
/// Implementors provide the core trading functionality for both backtesting and live trading.
#[async_trait]
pub trait TradeExecutor: Send + Sync {
    /// Opens a new long position with the specified size, leverage, and optional risk management
    /// parameters. Returns the UUID of the created trade.
    async fn open_long(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    ) -> TradeExecutorResult<Uuid>;

    /// Opens a new short position with the specified size, leverage, and optional risk management
    /// parameters. Returns the UUID of the created trade.
    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    ) -> TradeExecutorResult<Uuid>;

    /// Adds margin to an existing trade, reducing its leverage.
    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()>;

    /// Withdraws profit and/or margin from a running trade without closing the position.
    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()>;

    /// Closes a specific trade by its ID.
    async fn close_trade(&self, trade_id: Uuid) -> TradeExecutorResult<()>;

    /// Closes all long positions. Returns the UUIDs of the closed trades.
    async fn close_longs(&self) -> TradeExecutorResult<Vec<Uuid>>;

    /// Closes all short positions. Returns the UUIDs of the closed trades.
    async fn close_shorts(&self) -> TradeExecutorResult<Vec<Uuid>>;

    /// Closes all open positions (both long and short). Returns the UUIDs of the closed trades.
    async fn close_all(&self) -> TradeExecutorResult<Vec<Uuid>>;

    /// Returns the current trading state including balance, positions, and metrics.
    async fn trading_state(&self) -> TradeExecutorResult<TradingState>;
}

/// Trait for processing trading signals and making trading decisions.
///
/// Signal operators receive evaluated signals and determine when to open, close, or modify
/// positions. The type parameter `S` represents the signal type that this operator handles.
///
/// # Type Parameter
///
/// * `S` - The signal type this operator processes. This should match the signal type produced by
///   the evaluators being used.
#[async_trait]
pub trait SignalOperator<S: Signal>: Send + Sync {
    /// Sets the trade executor that should be used to execute trading operations.
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> GeneralResult<()>;

    /// Processes a trading signal and executes trading actions via the [`TradeExecutor`] that was
    /// set.
    async fn process_signal(&self, signal: &S) -> GeneralResult<()>;
}

pub(crate) struct WrappedSignalOperator<S: Signal>(Box<dyn SignalOperator<S>>);

impl<S: Signal> WrappedSignalOperator<S> {
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

    pub async fn process_signal(&self, signal: &S) -> TradeCoreResult<()> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.process_signal(signal)))
            .await
            .map_err(|e| TradeCoreError::SignalOperatorProcessSignalPanicked(e.into()))?
            .map_err(|e| TradeCoreError::SignalOperatorProcessSignalError(e.to_string()))
    }
}

impl<S: Signal> From<Box<dyn SignalOperator<S>>> for WrappedSignalOperator<S> {
    fn from(value: Box<dyn SignalOperator<S>>) -> Self {
        Self(value)
    }
}

/// Placeholder signal type for raw operators that don't use signals.
#[derive(Debug, Clone, Copy)]
pub struct Raw;

impl fmt::Display for Raw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Raw")
    }
}

/// Trait for implementing direct trading logic without intermediate signal generation. Raw operators
/// receive candlestick data and make trading decisions directly, providing more flexible control
/// over the trading strategy implementation.
#[async_trait]
pub trait RawOperator: Send + Sync {
    /// Sets the trade executor that should be used to execute trading operations.
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> GeneralResult<()>;

    /// Returns the lookback configuration for this operator, or `None` if no historical candle
    /// data is required.
    fn lookback(&self) -> Option<Lookback>;

    /// Returns the minimum interval between successive iterations of the operator.
    fn min_iteration_interval(&self) -> MinIterationInterval;

    /// Processes candlestick data and executes trading actions via the [`TradeExecutor`] that was
    /// set. Called periodically according to the minimum iteration interval.
    async fn iterate(&self, candles: &[OhlcCandleRow]) -> GeneralResult<()>;
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

    pub fn lookback(&self) -> TradeCoreResult<Option<Lookback>> {
        let lookback = panic::catch_unwind(AssertUnwindSafe(|| self.0.lookback()))
            .map_err(|e| TradeCoreError::RawOperatorLookbackPanicked(e.into()))?;
        Ok(lookback)
    }

    pub fn min_iteration_interval(&self) -> TradeCoreResult<MinIterationInterval> {
        let interval = panic::catch_unwind(AssertUnwindSafe(|| self.0.min_iteration_interval()))
            .map_err(|e| TradeCoreError::RawOperatorMinIterationIntervalPanicked(e.into()))?;
        Ok(interval)
    }

    pub async fn iterate(&self, candles: &[OhlcCandleRow]) -> TradeCoreResult<()> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.iterate(candles)))
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
    /// Calculates the price that must be reached to trigger a trailing stoploss update.
    ///
    /// For the stoploss to only trail in the favorable direction (UP for longs, DOWN for shorts),
    /// we must use division rather than multiplication in the trigger formula.
    ///
    /// For longs, when the trigger fires we calculate: `new_sl = trigger_price × (1 - tsl)`
    /// We want to guarantee: `new_sl >= curr_sl × (1 + step)`
    ///
    /// Solving for the trigger price:
    ///   `trigger_price × (1 - tsl) >= curr_sl × (1 + step)`
    ///   `trigger_price >= curr_sl × (1 + step) / (1 - tsl)`
    ///
    /// Note: We must use division `/ (1 - tsl)` rather than the simpler multiplication
    /// `× (1 + tsl)`, in order to guarantee that `new_sl >= next_stoploss`. Rounding errors
    /// from other methods may compound over updates, causing the stoploss to drift and
    /// potentially cross the liquidation price.
    fn next_stoploss_update_trigger(
        &self,
        tsl_step_size: PercentageCapped,
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
                let tsl_factor = 1.0 - trade_tsl.as_f64() / 100.0;
                let trigger_price = next_stoploss.as_f64() / tsl_factor;
                Price::round_up(trigger_price).map_err(|e| {
                    TradeCoreError::InvalidPriceRounding {
                        price: trigger_price,
                        e,
                    }
                })?
            }
            TradeSide::Sell => {
                let next_stoploss = curr_stoploss.apply_discount(tsl_step_size).map_err(|e| {
                    TradeCoreError::InvalidPriceApplyDiscount {
                        price: curr_stoploss,
                        discount: tsl_step_size,
                        e,
                    }
                })?;
                let tsl_factor = 1.0 + trade_tsl.as_f64() / 100.0;
                let trigger_price = next_stoploss.as_f64() / tsl_factor;
                Price::round_down(trigger_price).map_err(|e| {
                    TradeCoreError::InvalidPriceRounding {
                        price: trigger_price,
                        e,
                    }
                })?
            }
        };

        Ok(price_trigger)
    }

    fn eval_trigger_bounds(
        &self,
        tsl_step_size: PercentageCapped,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> TradeCoreResult<(Price, Price)> {
        let next_stoploss_update_trigger = trade_tsl
            .map(|tsl| self.next_stoploss_update_trigger(tsl_step_size, tsl))
            .transpose()?;

        match self.side() {
            TradeSide::Buy => {
                let lower_bound = self.stoploss().unwrap_or(self.liquidation());
                let takeprofit_trigger = self.takeprofit().unwrap_or(Price::MAX);
                let upper_bound =
                    takeprofit_trigger.min(next_stoploss_update_trigger.unwrap_or(Price::MAX));

                Ok((lower_bound, upper_bound))
            }
            TradeSide::Sell => {
                let takeprofit_trigger = self.takeprofit().unwrap_or(Price::MIN);
                let lower_bound =
                    takeprofit_trigger.max(next_stoploss_update_trigger.unwrap_or(Price::MIN));
                let upper_bound = self.stoploss().unwrap_or(self.liquidation());

                Ok((lower_bound, upper_bound))
            }
        }
    }

    fn was_closed_on_range(&self, range_min: f64, range_max: f64) -> bool {
        match self.side() {
            TradeSide::Buy => {
                let stoploss_reached = self
                    .stoploss()
                    .is_some_and(|stoploss| range_min <= stoploss.as_f64());
                let liquidation_reached = range_min <= self.liquidation().as_f64();
                let takeprofit_reached = self
                    .takeprofit()
                    .is_some_and(|takeprofit| range_max >= takeprofit.as_f64());

                stoploss_reached || liquidation_reached || takeprofit_reached
            }
            TradeSide::Sell => {
                let stoploss_reached = self
                    .stoploss()
                    .is_some_and(|stoploss| range_max >= stoploss.as_f64());
                let liquidation_reached = range_max >= self.liquidation().as_f64();
                let takeprofit_reached = self
                    .takeprofit()
                    .is_some_and(|takeprofit| range_min <= takeprofit.as_f64());

                stoploss_reached || liquidation_reached || takeprofit_reached
            }
        }
    }

    fn eval_new_stoploss_on_range(
        &self,
        tsl_step_size: PercentageCapped,
        trade_tsl: TradeTrailingStoploss,
        range_min: f64,
        range_max: f64,
    ) -> TradeCoreResult<Option<Price>> {
        let next_stoploss_update_trigger = self
            .next_stoploss_update_trigger(tsl_step_size, trade_tsl)?
            .as_f64();

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
        tsl_step_size: PercentageCapped,
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
                market_price <= min.as_f64() || market_price >= max.as_f64()
            }
        }
    }
}
