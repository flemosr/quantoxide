use std::{fmt, result::Result};

use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    error::{FuturesTradeRequestValidationError, QuantityValidationError},
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

/// A trade returned from the LNMarkets API.
///
/// Represents a complete trade object with all associated data including execution details, risk
/// parameters, lifecycle status, and profit/loss information.
///
/// # Examples
///
/// ```no_run
/// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::models::{
///     LnmTrade, TradeExecution, TradeSide, TradeSize, Leverage, Margin
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
    /// Returns the unique identifier for this trade.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let trade_id = trade.id();
    ///
    /// println!("Trade ID: {}", trade_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the user ID associated with this trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let user_id = trade.uid();
    ///
    /// println!("Trade belongs to user: {}", user_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn uid(&self) -> Uuid {
        self.uid
    }

    /// Returns the execution type (Market or Limit).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let exec_type = trade.trade_type();
    ///
    /// println!("Trade execution type: {:?}", exec_type);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
    }

    /// Returns the side of the trade (Buy or Sell).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let side = trade.side();
    ///
    /// println!("Trade side: {:?}", side);
    /// # Ok(())
    /// # }
    /// ```
    pub fn side(&self) -> TradeSide {
        self.side
    }

    /// Returns the opening fee charged when the trade was created (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = trade.opening_fee();
    ///
    /// println!("Opening fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    /// Returns the closing fee that will be charged when the trade closes (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = trade.closing_fee();
    ///
    /// println!("Closing fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    /// Returns the maintenance margin requirement (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let margin = trade.maintenance_margin();
    ///
    /// println!("Maintenance margin: {} sats", margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn maintenance_margin(&self) -> i64 {
        self.maintenance_margin
    }

    /// Returns the quantity (notional value in USD) of the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let quantity = trade.quantity();
    ///
    /// println!("Trade quantity: {}", quantity);
    /// # Ok(())
    /// # }
    /// ```
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the margin (collateral in satoshis) allocated to the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let margin = trade.margin();
    ///
    /// println!("Trade margin: {}", margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn margin(&self) -> Margin {
        self.margin
    }

    /// Returns the leverage multiplier applied to the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let leverage = trade.leverage();
    ///
    /// println!("Trade leverage: {}", leverage);
    /// # Ok(())
    /// # }
    /// ```
    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    /// Returns the trade price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let price = trade.price();
    ///
    /// println!("Trade price: {}", price);
    /// # Ok(())
    /// # }
    /// ```
    pub fn price(&self) -> Price {
        self.price
    }

    /// Returns the liquidation price at which the position will be automatically closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let liq_price = trade.liquidation();
    ///
    /// println!("Liquidation price: {}", liq_price);
    /// # Ok(())
    /// # }
    /// ```
    pub fn liquidation(&self) -> Price {
        self.liquidation
    }

    /// Returns the stop loss price, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(sl) = trade.stoploss() {
    ///     println!("Stop loss: {}", sl);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    /// Returns the take profit price, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(tp) = trade.takeprofit() {
    ///     println!("Take profit: {}", tp);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    /// Returns the price at which the trade was closed, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(exit) = trade.exit_price() {
    ///     println!("Exit price: {}", exit);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn exit_price(&self) -> Option<Price> {
        self.exit_price
    }

    /// Returns the realized profit/loss in satoshis.
    ///
    /// For running trades, this represents the current unrealized P/L. For closed trades, this is
    /// the final realized P/L.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
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

    /// Returns the timestamp when the trade was created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// let created_at = trade.creation_ts();
    ///
    /// println!("Trade created at: {}", created_at);
    /// # Ok(())
    /// # }
    /// ```
    pub fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts
    }

    /// Returns the timestamp when the trade was filled, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(filled_at) = trade.market_filled_ts() {
    ///     println!("Trade filled at: {}", filled_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts
    }

    /// Returns the timestamp when the trade was closed, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(closed_at) = trade.closed_ts() {
    ///     println!("Trade closed at: {}", closed_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts
    }

    /// Returns the actual entry price when the trade was filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(entry) = trade.entry_price() {
    ///     println!("Entry price: {}", entry);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    /// Returns the actual margin at entry, which may differ from the requested margin.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(entry_margin) = trade.entry_margin() {
    ///     println!("Entry margin: {}", entry_margin);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    /// Returns `true` if the trade is open (limit order not yet filled).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.open() {
    ///     println!("Trade is open (limit order not filled)");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(&self) -> bool {
        self.open
    }

    /// Returns `true` if the trade is currently running (filled and active).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.running() {
    ///     println!("Trade is actively running");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn running(&self) -> bool {
        self.running
    }

    /// Returns `true` if the trade was canceled before being filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.canceled() {
    ///     println!("Trade was canceled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn canceled(&self) -> bool {
        self.canceled
    }

    /// Returns `true` if the trade has been closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.closed() {
    ///     println!("Trade has been closed");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Returns the sum of all carry fees (funding fees) paid on this trade in satoshis.
    ///
    /// Carry fees are periodic funding payments charged on open positions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::models::LnmTrade) -> Result<(), Box<dyn std::error::Error>> {
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
