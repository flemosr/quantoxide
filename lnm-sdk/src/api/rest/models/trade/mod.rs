use std::{fmt, result::Result};

use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    error::{FuturesTradeRequestValidationError, QuantityValidationError, TradeValidationError},
    leverage::Leverage,
    margin::Margin,
    price::Price,
    quantity::Quantity,
    serde_util,
};

pub mod util;

/// The side of a trade position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

impl fmt::Display for TradeSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSide::Buy => "Buy".fmt(f),
            TradeSide::Sell => "Sell".fmt(f),
        }
    }
}

/// The size specification for a trade position.
///
/// Trade size can be specified either as a [`Quantity`] (notional value in USD) or as [`Margin`]
/// (collateral in satoshis). The API will calculate the corresponding value based on the provided
/// price and leverage.
///
/// # Examples
///
/// ```
/// use lnm_sdk::models::{TradeSize, Quantity, Margin};
///
/// // Specify size by quantity (USD notional value)
/// let size_by_quantity = TradeSize::from(Quantity::try_from(1_000).unwrap());
///
/// // Specify size by margin (satoshis collateral)
/// let size_by_margin = TradeSize::from(Margin::try_from(10_000).unwrap());
/// ```
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSize {
    #[serde(rename = "quantity")]
    Quantity(Quantity),
    #[serde(rename = "margin")]
    Margin(Margin),
}

impl TradeSize {
    /// Converts the trade size to both quantity and margin values.
    ///
    /// Calculates the corresponding quantity and margin based on the provided price and leverage.
    /// If the size is specified as margin, the quantity is calculated. If specified as quantity,
    /// the margin is calculated.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::{TradeSize, Quantity, Price, Leverage};
    ///
    /// let size = TradeSize::from(Quantity::try_from(1_000).unwrap());
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let leverage = Leverage::try_from(10.0).unwrap();
    ///
    /// let (quantity, margin) = size.to_quantity_and_margin(price, leverage).unwrap();
    /// ```
    pub fn to_quantity_and_margin(
        &self,
        price: Price,
        leverage: Leverage,
    ) -> Result<(Quantity, Margin), QuantityValidationError> {
        match self {
            TradeSize::Margin(margin) => {
                let quantity = Quantity::try_calculate(*margin, price, leverage)?;
                Ok((quantity, *margin))
            }
            TradeSize::Quantity(quantity) => {
                let margin = Margin::calculate(*quantity, price, leverage);
                Ok((*quantity, margin))
            }
        }
    }
}

impl From<Quantity> for TradeSize {
    fn from(quantity: Quantity) -> Self {
        TradeSize::Quantity(quantity)
    }
}

impl From<Margin> for TradeSize {
    fn from(margin: Margin) -> Self {
        TradeSize::Margin(margin)
    }
}

impl fmt::Display for TradeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSize::Quantity(quantity) => write!(f, "Quantity({})", quantity),
            TradeSize::Margin(margin) => write!(f, "Margin({})", margin),
        }
    }
}

/// The execution type of a trade.
///
/// Represents whether a trade is executed at market price or at a specific limit price.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecutionType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

/// The execution specification for a trade order.
///
/// Trades can be executed:
/// + Immediately at market price
/// + At a specific limit price
///
/// # Examples
///
/// ```
/// use lnm_sdk::models::{TradeExecution, Price};
///
/// // Execute immediately at market price
/// let market_execution = TradeExecution::Market;
///
/// // Execute only at or better than the specified price
/// let limit_execution = TradeExecution::Limit(Price::try_from(100_000.0).unwrap());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecution {
    Market,
    Limit(Price),
}

impl TradeExecution {
    /// Returns the execution type without the associated price data.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::{TradeExecution, TradeExecutionType, Price};
    ///
    /// let market_execution = TradeExecution::Market;
    /// assert!(matches!(market_execution.to_type(), TradeExecutionType::Market));
    ///
    /// let limit_execution = TradeExecution::Limit(Price::try_from(100_000.0).unwrap());
    /// assert!(matches!(limit_execution.to_type(), TradeExecutionType::Limit));
    /// ```
    pub fn to_type(&self) -> TradeExecutionType {
        match self {
            TradeExecution::Market => TradeExecutionType::Market,
            TradeExecution::Limit(_) => TradeExecutionType::Limit,
        }
    }
}

impl From<Price> for TradeExecution {
    fn from(price: Price) -> Self {
        Self::Limit(price)
    }
}

#[derive(Serialize, Debug)]
pub(crate) struct FuturesTradeRequestBody {
    leverage: Leverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    side: TradeSide,

    #[serde(flatten)]
    size: TradeSize,

    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss {
                if stoploss >= price {
                    return Err(FuturesTradeRequestValidationError::StopLossHigherThanPrice);
                }
            }

            if let Some(takeprofit) = takeprofit {
                if takeprofit <= price {
                    return Err(FuturesTradeRequestValidationError::TakeProfitLowerThanPrice);
                }
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        Ok(FuturesTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            size,
            trade_type,
            price,
        })
    }
}

/// The lifecycle status of a trade.
pub enum TradeStatus {
    Open,
    Running,
    Closed,
}

impl TradeStatus {
    /// Returns the status as a string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeStatus::Open => "open",
            TradeStatus::Running => "running",
            TradeStatus::Closed => "closed",
        }
    }

    /// Converts the status to an owned String.
    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

/// Core trait for trade implementations.
///
/// Provides access to common trade properties including identification, execution details,
/// risk management parameters, and lifecycle status. This trait is implemented by [`LnmTrade`].
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{
///     LnmTrade, Trade, TradeExecution, TradeSide, TradeSize, Leverage, Margin
/// };
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
/// println!("Trade ID: {}", trade.id());
/// println!("Side: {}", trade.side());
/// println!("Quantity: {}", trade.quantity());
/// println!("Margin: {}", trade.margin());
/// # Ok(())
/// # }
/// ```
pub trait Trade: Send + Sync + fmt::Debug + 'static {
    /// Returns the unique identifier for this trade.
    fn id(&self) -> Uuid;

    /// Returns the execution type (Market or Limit).
    fn trade_type(&self) -> TradeExecutionType;

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

    /// Returns the actual entry price when the trade was filled.
    fn entry_price(&self) -> Option<Price>;

    /// Returns the actual margin at entry, which may differ from the requested margin.
    fn entry_margin(&self) -> Option<Margin>;

    /// Returns `true` if the trade is open (limit order not yet filled).
    fn open(&self) -> bool;

    /// Returns `true` if the trade is currently running (filled and active).
    fn running(&self) -> bool;

    /// Returns `true` if the trade was canceled before being filled.
    fn canceled(&self) -> bool;

    /// Returns `true` if the trade has been closed.
    fn closed(&self) -> bool;
}

/// Extension trait for running trades with profit/loss and margin calculations.
///
/// Provides methods for estimating profit/loss and calculating margin adjustments for trades that
/// are currently active (running). This trait extends the [`Trade`] trait with functionality
/// specific to active positions.
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{
///     LnmTrade, TradeRunning, TradeExecution, TradeSide, TradeSize, Leverage,
///     Margin, Price
/// };
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
pub trait TradeRunning: Trade {
    /// Estimates the profit/loss for the trade at a given market price.
    ///
    /// Returns the estimated profit or loss in satoshis if the trade were closed at the specified
    /// market price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::models::{TradeRunning, Price};
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
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
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::models::TradeRunning;
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
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
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::models::{TradeRunning, Price};
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
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
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::models::{TradeRunning, Price};
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
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
        util::evaluate_collateral_delta_for_liquidation(
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

/// Extension trait for closed trades.
///
/// Provides access to the final profit/loss of a trade that has been closed. This trait extends the
/// [`Trade`] trait with functionality specific to completed positions.
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{TradeClosed, Trade};
/// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
/// # let trade = api.rest.futures.create_new_trade(
/// #     TradeSide::Buy,
/// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
/// #     Leverage::try_from(10.0).unwrap(),
/// #     TradeExecution::Market,
/// #     None,
/// #     None,
/// # ).await?;
///
/// let closed_trade = api.rest.futures.close_trade(trade.id()).await?;
///
/// let profit_loss = closed_trade.pl();
///
/// if profit_loss > 0 {
///     println!("Trade closed with profit: {} sats", profit_loss);
/// } else {
///     println!("Trade closed with loss: {} sats", profit_loss.abs());
/// }
/// # Ok(())
/// # }
/// ```
pub trait TradeClosed: Trade {
    /// Returns the realized profit/loss of the closed trade in satoshis.
    ///
    /// A positive value indicates profit, while a negative value indicates a loss.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::models::TradeClosed;
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin, Trade};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
    /// # let closed_trade = api.rest.futures.close_trade(trade.id()).await?;
    ///
    /// let pl = closed_trade.pl();
    ///
    /// println!("Realized P/L: {} sats", pl);
    /// # Ok(())
    /// # }
    /// ```
    fn pl(&self) -> i64;
}

/// A trade returned from the LNMarkets API.
///
/// Represents a complete trade object with all associated data including execution details, risk
/// parameters, lifecycle status, and profit/loss information. This is the concrete implementation
/// of the [`Trade`], [`TradeRunning`], and [`TradeClosed`] traits.
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{
///     LnmTrade, Trade, TradeExecution, TradeSide, TradeSize, Leverage, Margin
/// };
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
/// println!("Trade ID: {}", trade.id());
/// println!("User ID: {}", trade.uid());
/// println!("Side: {}", trade.side());
/// println!("Quantity: {} USD", trade.quantity());
/// println!("Margin: {} sats", trade.margin());
/// println!("Leverage: {}x", trade.leverage());
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
pub struct LnmTrade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: i64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    #[serde(with = "serde_util::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_ts: Option<DateTime<Utc>>,
    #[serde(with = "serde_util::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: i64,
}

impl LnmTrade {
    /// Returns the user ID associated with this trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
    /// let user_id = trade.uid();
    ///
    /// println!("Trade belongs to user: {}", user_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn uid(&self) -> Uuid {
        self.uid
    }

    /// Returns the realized profit/loss in satoshis.
    ///
    /// For running trades, this represents the current unrealized P/L. For closed trades, this is
    /// the final realized P/L.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
    /// let pl = trade.pl();
    ///
    /// if pl > 0 {
    ///     println!("Profit: {} sats", pl);
    /// } else {
    ///     println!("Loss: {} sats", pl.abs());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn pl(&self) -> i64 {
        self.pl
    }

    /// Returns the sum of all carry fees (funding fees) paid on this trade in satoshis.
    ///
    /// Carry fees are periodic funding payments charged on open positions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{TradeExecution, TradeSide, TradeSize, Leverage, Margin};
    /// # let trade = api.rest.futures.create_new_trade(
    /// #     TradeSide::Buy,
    /// #     TradeSize::from(Margin::try_from(10_000).unwrap()),
    /// #     Leverage::try_from(10.0).unwrap(),
    /// #     TradeExecution::Market,
    /// #     None,
    /// #     None,
    /// # ).await?;
    /// let total_fees = trade.sum_carry_fees();
    ///
    /// println!("Total carry fees paid: {} sats", total_fees);
    /// # Ok(())
    /// # }
    /// ```
    pub fn sum_carry_fees(&self) -> i64 {
        self.sum_carry_fees
    }
}

impl Trade for LnmTrade {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
    }

    fn side(&self) -> TradeSide {
        self.side
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    fn maintenance_margin(&self) -> i64 {
        self.maintenance_margin
    }

    fn quantity(&self) -> Quantity {
        self.quantity
    }

    fn margin(&self) -> Margin {
        self.margin
    }

    fn leverage(&self) -> Leverage {
        self.leverage
    }

    fn price(&self) -> Price {
        self.price
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    fn exit_price(&self) -> Option<Price> {
        self.exit_price
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts
    }

    fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    fn open(&self) -> bool {
        self.open
    }

    fn running(&self) -> bool {
        self.running
    }

    fn canceled(&self) -> bool {
        self.canceled
    }

    fn closed(&self) -> bool {
        self.closed
    }
}

impl TradeRunning for LnmTrade {
    fn est_pl(&self, market_price: Price) -> f64 {
        util::estimate_pl(self.side(), self.quantity(), self.price(), market_price)
    }
}

impl TradeClosed for LnmTrade {
    fn pl(&self) -> i64 {
        self.pl
    }
}

#[derive(Deserialize)]
pub(crate) struct NestedTradesResponse {
    pub trades: Vec<LnmTrade>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TradeUpdateType {
    Stoploss,
    Takeprofit,
}

#[derive(Serialize, Debug)]
pub(crate) struct FuturesUpdateTradeRequestBody {
    id: Uuid,
    #[serde(rename = "type")]
    update_type: TradeUpdateType,
    value: Price,
}

impl FuturesUpdateTradeRequestBody {
    pub fn new(id: Uuid, update_type: TradeUpdateType, value: Price) -> Self {
        Self {
            id,
            update_type,
            value,
        }
    }
}
