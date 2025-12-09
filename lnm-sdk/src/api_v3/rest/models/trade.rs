use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::models::{
    leverage::Leverage,
    margin::Margin,
    price::Price,
    quantity::Quantity,
    serde_util,
    trade::{TradeExecution, TradeExecutionType, TradeSide, TradeSize},
};

use super::{
    cross_leverage::CrossLeverage,
    error::{FuturesCrossTradeOrderValidationError, FuturesIsolatedTradeRequestValidationError},
};

#[derive(Serialize, Debug)]
pub(in crate::api_v3) struct FuturesIsolatedTradeRequestBody {
    leverage: Leverage,
    side: TradeSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(flatten)]
    size: TradeSize,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesIsolatedTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        client_id: Option<String>,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesIsolatedTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss
                && stoploss >= price
            {
                return Err(FuturesIsolatedTradeRequestValidationError::StopLossHigherThanPrice);
            }

            if let Some(takeprofit) = takeprofit
                && takeprofit <= price
            {
                return Err(FuturesIsolatedTradeRequestValidationError::TakeProfitLowerThanPrice);
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        if client_id
            .as_ref()
            .is_some_and(|client_id| client_id.len() > 64)
        {
            return Err(FuturesIsolatedTradeRequestValidationError::ClientIdTooLong);
        }

        Ok(FuturesIsolatedTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            client_id,
            size,
            trade_type,
            price,
        })
    }
}

/// An isolated futures trade returned from the LN Markets API.
///
/// Represents a complete isolated trade object with all associated data including execution
/// details, risk parameters, lifecycle status, and profit/loss information. Unlike cross-margin
/// positions, each isolated trade has its own dedicated margin and risk parameters.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{
///     Trade, Leverage, Margin, TradeExecution, TradeSide, TradeSize,
/// };
///
/// let trade: Trade = rest_api
///     .futures_isolated
///     .new_trade(
///         TradeSide::Buy,
///         TradeSize::from(Margin::try_from(10_000)?),
///         Leverage::try_from(10.0)?,
///         TradeExecution::Market,
///         None,
///         None,
///         None,
///     )
///     .await?;
///
/// println!("Trade ID: {}", trade.id());
/// println!("Side: {:?}", trade.side());
/// println!("Quantity: {}", trade.quantity());
/// println!("Margin: {}", trade.margin());
/// println!("Leverage: {}", trade.leverage());
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    id: Uuid,
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
    created_at: DateTime<Utc>,
    filled_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
    #[serde(with = "serde_util::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_funding_fees: i64,
    client_id: Option<String>,
}

impl Trade {
    /// Returns the unique identifier for this trade.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let trade_id = trade.id();
    ///
    /// println!("Trade ID: {}", trade_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the execution type (Market or Limit).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let side = trade.side();
    ///
    /// println!("Trade side: {:?}", side);
    /// # Ok(())
    /// # }
    /// ```
    pub fn side(&self) -> TradeSide {
        self.side
    }

    /// Returns the opening fee charged when the trade was filled (in satoshis), or zero if the
    /// trade was not filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = trade.opening_fee();
    ///
    /// println!("Opening fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    /// Returns the closing fee that was charged when the trade was closed (in satoshis), or zero
    /// if the trade was not closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let created_at = trade.created_at();
    ///
    /// println!("Trade created at: {}", created_at);
    /// # Ok(())
    /// # }
    /// ```
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the timestamp when the trade was filled, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(filled_at) = trade.filled_at() {
    ///     println!("Trade filled at: {}", filled_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn filled_at(&self) -> Option<DateTime<Utc>> {
        self.filled_at
    }

    /// Returns the timestamp when the trade was closed, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(closed_at) = trade.closed_at() {
    ///     println!("Trade closed at: {}", closed_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed_at(&self) -> Option<DateTime<Utc>> {
        self.closed_at
    }

    /// Returns the actual entry price when the trade was filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.closed() {
    ///     println!("Trade has been closed");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Returns the sum of all funding fees paid on this trade in satoshis.
    ///
    /// Funding fees are periodic payments charged on open orders.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let total_fees = trade.sum_funding_fees();
    ///
    /// println!("Total funding fees paid: {} sats", total_fees);
    /// # Ok(())
    /// # }
    /// ```
    pub fn sum_funding_fees(&self) -> i64 {
        self.sum_funding_fees
    }

    /// Returns the client-provided identifier for this trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(client_id) = trade.client_id() {
    ///     println!("Client ID: {}", client_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_id(&self) -> Option<&String> {
        self.client_id.as_ref()
    }
}

#[derive(Serialize, Debug)]
pub(in crate::api_v3) struct FuturesCrossOrderBody {
    side: TradeSide,
    quantity: Quantity,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}

impl FuturesCrossOrderBody {
    pub fn new(
        side: TradeSide,
        quantity: Quantity,
        execution: TradeExecution,
        client_id: Option<String>,
    ) -> Result<Self, FuturesCrossTradeOrderValidationError> {
        let (trade_type, price) = match execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        if client_id
            .as_ref()
            .is_some_and(|client_id| client_id.len() > 64)
        {
            return Err(FuturesCrossTradeOrderValidationError::ClientIdTooLong);
        }

        Ok(FuturesCrossOrderBody {
            side,
            quantity,
            trade_type,
            price,
            client_id,
        })
    }
}

/// An order to modify a cross-margin futures position returned from the LN Markets API.
///
/// Represents an order that, when filled, will update the user's [`CrossPosition`]. Cross orders
/// allow traders to increase or decrease their position size within a unified cross-margin account,
/// where margin is shared across all positions.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{
///     CrossOrder, Quantity, TradeExecution, TradeSide,
/// };
///
/// let order: CrossOrder = rest_api
///     .futures_cross
///     .place_order(
///         TradeSide::Buy,
///         Quantity::try_from(1000)?,
///         TradeExecution::Market,
///         None,
///     )
///     .await?;
///
/// println!("Order ID: {}", order.id());
/// println!("Side: {:?}", order.side());
/// println!("Quantity: {}", order.quantity());
/// println!("Status - Open: {}, Filled: {}", order.open(), order.filled());
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrossOrder {
    id: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    quantity: Quantity,
    price: Price,
    trading_fee: u64,
    created_at: DateTime<Utc>,
    filled_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
    open: bool,
    filled: bool,
    canceled: bool,
    client_id: Option<String>,
}

impl CrossOrder {
    /// Returns the unique identifier for this cross order.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let order_id = order.id();
    ///
    /// println!("Order ID: {}", order_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the execution type (Market or Limit).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let exec_type = order.trade_type();
    ///
    /// println!("Order execution type: {:?}", exec_type);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
    }

    /// Returns the side of the order (Buy or Sell).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let side = order.side();
    ///
    /// println!("Order side: {:?}", side);
    /// # Ok(())
    /// # }
    /// ```
    pub fn side(&self) -> TradeSide {
        self.side
    }

    /// Returns the quantity (notional value in USD) of the order.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let quantity = order.quantity();
    ///
    /// println!("Order quantity: {}", quantity);
    /// # Ok(())
    /// # }
    /// ```
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the order price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let price = order.price();
    ///
    /// println!("Order price: {}", price);
    /// # Ok(())
    /// # }
    /// ```
    pub fn price(&self) -> Price {
        self.price
    }

    /// Returns the trading fee charged (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = order.trading_fee();
    ///
    /// println!("Trading fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trading_fee(&self) -> u64 {
        self.trading_fee
    }

    /// Returns the timestamp when the order was created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// let created_at = order.created_at();
    ///
    /// println!("Order created at: {}", created_at);
    /// # Ok(())
    /// # }
    /// ```
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the timestamp when the order was filled, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(filled_at) = order.filled_at() {
    ///     println!("Order filled at: {}", filled_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn filled_at(&self) -> Option<DateTime<Utc>> {
        self.filled_at
    }

    /// Returns the timestamp when the order was canceled, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(canceled_at) = order.canceled_at() {
    ///     println!("Order canceled at: {}", canceled_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn canceled_at(&self) -> Option<DateTime<Utc>> {
        self.canceled_at
    }

    /// Returns `true` if the order is open (limit order not yet filled).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if order.open() {
    ///     println!("Order is open (limit order not filled)");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(&self) -> bool {
        self.open
    }

    /// Returns `true` if the order has been filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if order.filled() {
    ///     println!("Order has been filled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn filled(&self) -> bool {
        self.filled
    }

    /// Returns `true` if the order was canceled before being filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if order.canceled() {
    ///     println!("Order was canceled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn canceled(&self) -> bool {
        self.canceled
    }

    /// Returns the client-provided identifier for this order.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(order: lnm_sdk::api_v3::models::CrossOrder) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(client_id) = order.client_id() {
    ///     println!("Client ID: {}", client_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_id(&self) -> Option<&String> {
        self.client_id.as_ref()
    }
}

/// A cross-margin futures position returned from the LN Markets API.
///
/// Represents a user's aggregated cross-margin position where margin is shared across the entire
/// account rather than allocated per trade.
///
/// Cross positions are modified through [`CrossOrder`]s.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::CrossPosition;
///
/// let position: CrossPosition = rest_api
///     .futures_cross
///     .get_position()
///     .await?;
///
/// println!("Position ID: {}", position.id());
/// println!("Quantity: {}", position.quantity());
/// println!("Margin: {}", position.margin());
/// println!("Leverage: {}", position.leverage());
/// if let Some(entry_price) = position.entry_price() {
///     println!("Entry price: {}", entry_price);
/// }
/// println!("Total P/L: {} sats", position.total_pl());
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrossPosition {
    id: Uuid,
    margin: u64,
    quantity: u64,
    leverage: CrossLeverage,
    entry_price: Option<Price>,
    running_margin: u64,
    initial_margin: u64,
    maintenance_margin: u64,
    liquidation: Option<Price>,
    trading_fees: u64,
    funding_fees: u64,
    total_pl: i64,
    delta_pl: i64,
}

impl CrossPosition {
    /// Returns the unique identifier for this cross position.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let position_id = position.id();
    ///
    /// println!("Position ID: {}", position_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the margin (collateral in satoshis) allocated to the position.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let margin = position.margin();
    ///
    /// println!("Position margin: {}", margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn margin(&self) -> u64 {
        self.margin
    }

    /// Returns the quantity (notional value in USD) of the position.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let quantity = position.quantity();
    ///
    /// println!("Position quantity: {}", quantity);
    /// # Ok(())
    /// # }
    /// ```
    pub fn quantity(&self) -> u64 {
        self.quantity
    }

    /// Returns the leverage multiplier applied to the position.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let leverage = position.leverage();
    ///
    /// println!("Position leverage: {}", leverage);
    /// # Ok(())
    /// # }
    /// ```
    pub fn leverage(&self) -> CrossLeverage {
        self.leverage
    }

    /// Returns the entry price of the position, if any.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(entry_price) = position.entry_price() {
    ///     println!("Entry price: {}", entry_price);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    /// Returns the running margin (current margin including P/L) in satoshis.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let running_margin = position.running_margin();
    ///
    /// println!("Running margin: {}", running_margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn running_margin(&self) -> u64 {
        self.running_margin
    }

    /// Returns the initial margin of the position (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let initial_margin = position.initial_margin();
    ///
    /// println!("Initial margin: {}", initial_margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn initial_margin(&self) -> u64 {
        self.initial_margin
    }

    /// Returns the maintenance margin requirement (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let maintenance_margin = position.maintenance_margin();
    ///
    /// println!("Maintenance margin: {}", maintenance_margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn maintenance_margin(&self) -> u64 {
        self.maintenance_margin
    }

    /// Returns the liquidation price at which the position will be automatically closed, if any.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(liq_price) = position.liquidation() {
    ///     println!("Liquidation price: {}", liq_price);
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn liquidation(&self) -> Option<Price> {
        self.liquidation
    }

    /// Returns the total trading fees paid on this position in satoshis.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let fees = position.trading_fees();
    ///
    /// println!("Trading fees: {} sats", fees);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trading_fees(&self) -> u64 {
        self.trading_fees
    }

    /// Returns the net funding fees for this position in satoshis.
    ///
    /// Funding fees are periodic payments that can be either paid by the user (positive value)
    /// or received by the user (negative value), depending on the funding rate. The funding rate
    /// varies according to the current balance between long and short positions on the exchange.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let fees = position.funding_fees();
    ///
    /// println!("Funding fees: {} sats", fees);
    /// # Ok(())
    /// # }
    /// ```
    pub fn funding_fees(&self) -> u64 {
        self.funding_fees
    }

    /// Returns the total profit/loss in satoshis.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let total_pl = position.total_pl();
    ///
    /// if total_pl > 0 {
    ///     println!("Total profit: {} sats", total_pl);
    /// } else {
    ///     println!("Total loss: {} sats", total_pl.abs());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn total_pl(&self) -> i64 {
        self.total_pl
    }

    /// Returns the delta profit/loss in satoshis since last update.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(position: lnm_sdk::api_v3::models::CrossPosition) -> Result<(), Box<dyn std::error::Error>> {
    /// let delta_pl = position.delta_pl();
    ///
    /// if delta_pl > 0 {
    ///     println!("P/L change: +{} sats", delta_pl);
    /// } else {
    ///     println!("P/L change: {} sats", delta_pl);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn delta_pl(&self) -> i64 {
        self.delta_pl
    }
}
