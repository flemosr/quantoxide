use std::num::NonZeroU64;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::{
    models::{
        leverage::Leverage,
        price::Price,
        quantity::Quantity,
        trade::{TradeExecution, TradeSide, TradeSize},
    },
    rest::error::Result,
};

use super::models::{
    account::Account,
    cross_leverage::CrossLeverage,
    funding::{CrossFunding, FundingSettlement, IsolatedFunding},
    ohlc_candle::{OhlcCandle, OhlcRange},
    oracle::{Index, LastPrice},
    page::Page,
    ticker::Ticker,
    trade::{CrossOrder, CrossPosition, Trade},
    transfer::CrossTransfer,
};

/// Methods for interacting with [LNM's v3 API]'s REST Utilities endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait UtilitiesRepository: crate::sealed::Sealed + Send + Sync {
    /// Ping.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// rest.utilities.ping().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn ping(&self) -> Result<()>;

    /// Get the server time.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use chrono::{DateTime, Utc};
    ///
    /// let server_time: DateTime<Utc> = rest.utilities.time().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn time(&self) -> Result<DateTime<Utc>>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Isolated endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait FuturesIsolatedRepository: crate::sealed::Sealed + Send + Sync {
    /// Add margin to a running trade. This will lower the trade liquidation price and thus decrease
    /// risk.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use std::num::NonZeroU64;
    /// # use uuid::Uuid;
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let amount = NonZeroU64::try_from(1000)?;
    /// let updated_trade: Trade = rest
    ///     .futures_isolated
    ///     .add_margin_to_trade(trade_id, amount)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn add_margin_to_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade>;

    /// Cancel all open trades.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// let cancelled_trades: Vec<Trade> = rest.futures_isolated.cancel_all_trades().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_all_trades(&self) -> Result<Vec<Trade>>;

    /// Cancel an open trade.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use uuid::Uuid;
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let cancelled_trade: Trade = rest.futures_isolated.cancel_trade(trade_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_trade(&self, id: Uuid) -> Result<Trade>;

    /// Cash-in (i.e. "remove money") from a trade. Funds are first removed from the trade's PL (if
    /// any), then from the trade's margin. Note that cashing-in increases the trade's leverage; the
    /// whole margin hence isn't available since leverage is bounded.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use std::num::NonZeroU64;
    /// # use uuid::Uuid;
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let amount = NonZeroU64::try_from(500)?;
    /// let updated_trade: Trade = rest
    ///     .futures_isolated
    ///     .cash_in_trade(trade_id, amount)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn cash_in_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade>;

    /// Close a running trade and realize the PL.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use uuid::Uuid;
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let closed_trade: Trade = rest.futures_isolated.close_trade(trade_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn close_trade(&self, id: Uuid) -> Result<Trade>;

    /// Get all the trades that are still open.
    ///
    /// **Required permissions**: `futures:isolated:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// let open_trades: Vec<Trade> = rest.futures_isolated.get_open_trades().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_open_trades(&self) -> Result<Vec<Trade>>;

    /// Get all the trades that are running.
    ///
    /// **Required permissions**: `futures:isolated:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Trade;
    ///
    /// let running_trades: Vec<Trade> = rest.futures_isolated.get_running_trades().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_running_trades(&self) -> Result<Vec<Trade>>;

    /// Get closed trades.
    ///
    /// **Required permissions**: `futures:isolated:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{Page, Trade};
    ///
    /// let closed_trades: Page<Trade> = rest
    ///     .futures_isolated
    ///     .get_closed_trades(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_closed_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<Trade>>;

    /// Get canceled trades.
    ///
    /// **Required permissions**: `futures:isolated:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{Page, Trade};
    ///
    /// let canceled_trades: Page<Trade> = rest
    ///     .futures_isolated
    ///     .get_canceled_trades(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_canceled_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<Trade>>;

    /// Update an open or running trade takeprofit. If the provided `value` is `None`, the
    /// takeprofit will be removed.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{Price, Trade};
    /// # use uuid::Uuid;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let new_takeprofit = Some(Price::try_from(110_000)?);
    /// let updated_trade: Trade = rest
    ///     .futures_isolated
    ///     .update_takeprofit(trade_id, new_takeprofit)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn update_takeprofit(&self, id: Uuid, value: Option<Price>) -> Result<Trade>;

    /// Update an open or running trade stoploss. If the provided `value` is `None`, the stoploss
    /// will be removed.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{Price, Trade};
    /// # use uuid::Uuid;
    ///
    /// # let trade_id = Uuid::new_v4();
    /// let new_stoploss = Some(Price::try_from(90_000)?);
    /// let updated_trade: Trade = rest
    ///     .futures_isolated
    ///     .update_stoploss(trade_id, new_stoploss)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn update_stoploss(&self, id: Uuid, value: Option<Price>) -> Result<Trade>;

    /// Place a new isolated trade.
    ///
    /// **Required permissions**: `futures:isolated:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{
    ///     Leverage, Price, Quantity, Trade, TradeExecution, TradeSide, TradeSize,
    /// };
    ///
    /// // Create a long market order with 100 USD quantity and 2x leverage
    /// let trade: Trade = rest
    ///     .futures_isolated
    ///     .new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::Quantity(Quantity::try_from(100)?),
    ///         Leverage::try_from(2)?,
    ///         TradeExecution::Market,
    ///         None,
    ///         None,
    ///         None,
    ///     )
    ///     .await?;
    ///
    /// // Create a short limit order at 105,000 USD/BTC with stoploss and takeprofit
    /// let trade: Trade = rest
    ///     .futures_isolated
    ///     .new_trade(
    ///         TradeSide::Sell,
    ///         TradeSize::Quantity(Quantity::try_from(50)?),
    ///         Leverage::try_from(3)?,
    ///         TradeExecution::Limit(Price::try_from(105_000)?),
    ///         Some(Price::try_from(110_000)?),
    ///         Some(Price::try_from(100_000)?),
    ///         None,
    ///     )
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_arguments)]
    async fn new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        client_id: Option<String>,
    ) -> Result<Trade>;

    /// Get the funding fees paid for all the isolated trades, or for a specific trade.
    ///
    /// **Required permissions**: `futures:isolated:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{IsolatedFunding, Page};
    ///
    /// let funding_fees: Page<IsolatedFunding> = rest
    ///     .futures_isolated
    ///     .get_funding_fees(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_funding_fees(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<IsolatedFunding>>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Cross endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait FuturesCrossRepository: crate::sealed::Sealed + Send + Sync {
    /// Cancel all open cross orders.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::CrossOrder;
    ///
    /// let cancelled_orders: Vec<CrossOrder> = rest.futures_cross.cancel_all_orders().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_all_orders(&self) -> Result<Vec<CrossOrder>>;

    /// Cancel an open cross order.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use uuid::Uuid;
    /// use lnm_sdk::api_v3::models::CrossOrder;
    ///
    /// # let order_id = Uuid::new_v4();
    /// let cancelled_order: CrossOrder = rest.futures_cross.cancel_order(order_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_order(&self, id: Uuid) -> Result<CrossOrder>;

    /// Place a new cross order.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{CrossOrder, Price, Quantity, TradeExecution, TradeSide};
    ///
    /// // Place a market buy order for 50 USD
    /// let order: CrossOrder = rest
    ///     .futures_cross
    ///     .place_order(
    ///         TradeSide::Buy,
    ///         Quantity::try_from(50)?,
    ///         TradeExecution::Market,
    ///         None,
    ///     )
    ///     .await?;
    ///
    /// // Place a limit sell order at 105,000 USD/BTC for 100 USD
    /// let order: CrossOrder = rest
    ///     .futures_cross
    ///     .place_order(
    ///         TradeSide::Sell,
    ///         Quantity::try_from(100)?,
    ///         TradeExecution::Limit(Price::try_from(105_000)?),
    ///         None,
    ///     )
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn place_order(
        &self,
        side: TradeSide,
        quantity: Quantity,
        execution: TradeExecution,
        client_id: Option<String>,
    ) -> Result<CrossOrder>;

    /// Get all the cross orders that are still open.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::CrossOrder;
    ///
    /// let open_orders: Vec<CrossOrder> = rest.futures_cross.get_open_orders().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_open_orders(&self) -> Result<Vec<CrossOrder>>;

    /// Get the current cross margin position.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::CrossPosition;
    ///
    /// let position: CrossPosition = rest.futures_cross.get_position().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_position(&self) -> Result<CrossPosition>;

    /// Get the cross orders that have been filled.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{CrossOrder, Page};
    ///
    /// let filled_orders: Page<CrossOrder> = rest
    ///     .futures_cross
    ///     .get_filled_orders(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_filled_orders(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<CrossOrder>>;

    /// Close the running cross margin position. This will pass a market order opposite to the
    /// current position.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::CrossOrder;
    ///
    /// let closing_order: CrossOrder = rest.futures_cross.close_position().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn close_position(&self) -> Result<CrossOrder>;

    /// Get the funding fees paid for the cross margin position.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{CrossFunding, Page};
    ///
    /// let funding_fees: Page<CrossFunding> = rest
    ///     .futures_cross
    ///     .get_funding_fees(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_funding_fees(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<CrossFunding>>;

    /// Get the transfers history for the cross margin position (deposits to and withdrawals from
    /// the cross margin account). Positive amounts are deposits, negative amounts are withdrawals.
    ///
    /// **Required permissions**: `futures:cross:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{CrossTransfer, Page};
    ///
    /// let transfers: Page<CrossTransfer> = rest
    ///     .futures_cross
    ///     .get_transfers(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_transfers(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<CrossTransfer>>;

    /// Deposit funds to the cross margin account.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use std::num::NonZeroU64;
    /// use lnm_sdk::api_v3::models::CrossPosition;
    ///
    /// let amount = NonZeroU64::try_from(10_000)?;
    /// let position: CrossPosition = rest.futures_cross.deposit(amount).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn deposit(&self, amount: NonZeroU64) -> Result<CrossPosition>;

    /// Set the leverage of the cross margin position. If the available margin is not enough to
    /// cover the new position, some of the PL will be realized to cover the difference if possible.
    /// Returns the updated position.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{CrossLeverage, CrossPosition};
    ///
    /// let leverage = CrossLeverage::try_from(5)?;
    /// let position: CrossPosition = rest.futures_cross.set_leverage(leverage).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn set_leverage(&self, leverage: CrossLeverage) -> Result<CrossPosition>;

    /// Withdraw funds from the cross margin account.
    ///
    /// **Required permissions**: `futures:cross:write`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use std::num::NonZeroU64;
    /// use lnm_sdk::api_v3::models::CrossPosition;
    ///
    /// let amount = NonZeroU64::try_from(5_000)?;
    /// let position: CrossPosition = rest.futures_cross.withdraw(amount).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn withdraw(&self, amount: NonZeroU64) -> Result<CrossPosition>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Data endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait FuturesDataRepository: crate::sealed::Sealed + Send + Sync {
    /// Get the funding settlement history. A settlement happens every 8 hours (00:00, 08:00,
    /// 16:00 UTC).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{FundingSettlement, Page};
    ///
    /// let funding_settlements: Page<FundingSettlement> = rest
    ///     .futures_data
    ///     .get_funding_settlements(None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_funding_settlements(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<FundingSettlement>>;

    /// Get the futures ticker.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Ticker;
    ///
    /// let ticker: Ticker = rest.futures_data.get_ticker().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_ticker(&self) -> Result<Ticker>;

    /// Get the candles (OHLCs) history for a given range.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::{OhlcCandle, Page};
    ///
    /// let candles: Page<OhlcCandle> = rest
    ///     .futures_data
    ///     .get_candles(None, None, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_candles(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        range: Option<OhlcRange>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<OhlcCandle>>;

    // /// Get the 10 first users by P&L, broken down by day/week/month/all-time.
    // async fn get_leaderboard(&self) -> Result<()> {
    //     todo!()
    // }
}

/// Methods for interacting with [LNM's v3 API]'s REST Synthetic USD endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[allow(dead_code)] // TODO
#[async_trait]
pub trait SyntheticUsdRepository: crate::sealed::Sealed + Send + Sync {
    /// Fetch the user's swaps.
    ///
    /// **Required permissions**: `synthetic-usd:read`
    async fn get_swaps(&self) -> Result<()> {
        todo!()
    }

    /// Create a new swap.
    ///
    /// **Required permissions**: `synthetic-usd:write`
    async fn create_new_swap(&self) -> Result<()> {
        todo!()
    }

    /// Get best price.
    async fn get_best_price(&self) -> Result<()> {
        todo!()
    }
}

/// Methods for interacting with [LNM's v3 API]'s REST Account endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait AccountRepository: crate::sealed::Sealed + Send + Sync {
    /// Get account information.
    ///
    /// **Required permissions**: `account:read`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Account;
    ///
    /// let account: Account = rest.account.get_account().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_account(&self) -> Result<Account>;

    // /// Get the most recently generated, still unused on-chain address.
    // ///
    // /// **Required permissions**: `account:deposits:read`
    // async fn get_last_unused_onchain_address(&self) -> Result<()> {
    //     todo!()
    // }

    // /// Generates a new, unused, Bitcoin address. If no format is provided, the address will be
    // /// generated in the format specified in the user's settings.
    // ///
    // /// **Required permissions**: `account:deposits:write`
    // async fn generate_new_bitcoin_address(&self) -> Result<()> {
    //     todo!()
    // }

    // /// Get notifications for the current user. By default returns unread notifications. Use the
    // /// read parameter to filter by read status.
    // ///
    // /// **Required permissions**: `account:notifications:read`
    // async fn get_notifications(&self) -> Result<()> {
    //     todo!()
    // }

    // /// Mark all notifications as read for the current user.
    // ///
    // /// **Required permissions**: `account:notifications:write`
    // async fn mark_notifications_read(&self) -> Result<()> {
    //     todo!()
    // }
}

/// Methods for interacting with [LNM's v3 API]'s REST Deposits endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[allow(dead_code)] // TODO
#[async_trait]
pub trait DepositsRepository: crate::sealed::Sealed + Send + Sync {
    /// Get internal deposits.
    ///
    /// **Required permissions**: `account:deposits:read`
    async fn get_internal_deposits(&self) -> Result<()> {
        todo!()
    }

    /// Get on-chain deposits.
    ///
    /// **Required permissions**: `account:deposits:read`
    async fn get_onchain_deposits(&self) -> Result<()> {
        todo!()
    }

    /// Get Lightning deposits.
    ///
    /// **Required permissions**: `account:deposits:read`
    async fn get_lightning_deposits(&self) -> Result<()> {
        todo!()
    }

    /// Initiates a new Lightning deposit.
    ///
    /// **Required permissions**: `account:deposits:write`
    async fn deposit(&self) -> Result<()> {
        todo!()
    }
}

/// Methods for interacting with [LNM's v3 API]'s REST Withdrawals endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[allow(dead_code)] // TODO
#[async_trait]
pub trait WithdrawalsRepository: crate::sealed::Sealed + Send + Sync {
    /// Get internal withdrawals.
    ///
    /// **Required permissions**: `account:withdrawals:read`
    async fn get_internal_withdrawals(&self) -> Result<()> {
        todo!()
    }

    /// Get multiple on-chain withdrawals.
    ///
    /// **Required permissions**: `account:withdrawals:read`
    async fn get_onchain_withdrawals(&self) -> Result<()> {
        todo!()
    }

    /// Get multiple Lightning withdrawals.
    ///
    /// **Required permissions**: `account:withdrawals:read`
    async fn get_lightning_withdrawals(&self) -> Result<()> {
        todo!()
    }

    /// Create a new internal withdrawal.
    ///
    /// **Required permissions**: `account:withdrawals:write`
    async fn withdrawal_internal(&self) -> Result<()> {
        todo!()
    }

    /// Request a new on-chain withdrawal. The withdrawal request will be reviewed and processed
    /// asynchronously.
    ///
    /// **Required permissions**: `account:withdrawals:write`
    async fn withdrawal_onchain(&self) -> Result<()> {
        todo!()
    }

    /// Request a new Lightning withdrawal. The `max_fees` amount will be reserved from the user's
    /// balance to pay routing fees. Any unused portion of this reserve will be returned to the
    /// user's balance after the withdrawal completes.
    ///
    /// **Required permissions**: `account:withdrawals:write`
    async fn withdrawal_lightning(&self) -> Result<()> {
        todo!()
    }
}

/// Methods for interacting with [LNM's v3 API]'s REST Oracle endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3/
#[async_trait]
pub trait OracleRepository: crate::sealed::Sealed + Send + Sync {
    /// Samples index history (default 100, max 1000 entries).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::Index;
    ///
    /// let index: Vec<Index> = rest.oracle.get_index(None, None, None, None).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_index(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Vec<Index>>;

    /// Samples last price history at most 1000 entries between two given timestamps.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// use lnm_sdk::api_v3::models::LastPrice;
    ///
    /// let last_price: Vec<LastPrice> = rest.oracle.get_last_price(None, None, None, None).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_last_price(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Vec<LastPrice>>;
}
