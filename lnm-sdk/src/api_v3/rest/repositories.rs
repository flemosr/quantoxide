use std::num::NonZeroU64;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::{
    models::{
        leverage::Leverage,
        price::Price,
        trade::{TradeExecution, TradeSide, TradeSize},
    },
    rest::error::Result,
};

use super::models::{
    ticker::Ticker,
    trade::{PaginatedTrades, Trade},
};

/// Methods for interacting with [LNM's v3 API]'s REST Utilities endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait UtilitiesRepository: crate::sealed::Sealed + Send + Sync {
    async fn ping(&self) -> Result<()>;

    async fn time(&self) -> Result<DateTime<Utc>>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Isolated endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesIsolatedRepository: crate::sealed::Sealed + Send + Sync {
    /// Add margin to a running trade. This will lower the trade liquidation price and thus decrease
    /// risk.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn add_margin_to_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade>;

    /// Cancel all open trades.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn cancel_all_trades(&self) -> Result<Vec<Trade>>;

    /// Cancel an open trade.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn cancel_trade(&self, id: Uuid) -> Result<Trade>;

    /// Cash-in (i.e. "remove money") from a trade. Funds are first removed from the trade's PL (if
    /// any), then from the trade's margin. Note that cashing-in increases the trade's leverage; the
    /// whole margin hence isn't available since leverage is bounded.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn cash_in_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade>;

    /// Close a running trade and realize the PL.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn close_trade(&self, id: Uuid) -> Result<Trade>;

    /// Get all the trades that are still open.
    ///
    /// **Required permissions**: `futures:isolated:read`
    async fn get_open_trades(&self) -> Result<Vec<Trade>>;

    /// Get all the trades that are running.
    ///
    /// **Required permissions**: `futures:isolated:read`
    async fn get_running_trades(&self) -> Result<Vec<Trade>>;

    /// Get closed trades.
    ///
    /// **Required permissions**: `futures:isolated:read`
    async fn get_closed_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<PaginatedTrades>;

    /// Get canceled trades.
    ///
    /// **Required permissions**: `futures:isolated:read`
    async fn get_canceled_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<PaginatedTrades>;

    /// Update an open or running trade takeprofit. If the provided `value` is `None`, the
    /// takeprofit will be removed.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn update_takeprofit(&self, id: Uuid, value: Option<Price>) -> Result<Trade>;

    /// Update an open or running trade stoploss. If the provided `value` is `None`, the stoploss
    /// will be removed.
    ///
    /// **Required permissions**: `futures:isolated:write`
    async fn update_stoploss(&self, id: Uuid, value: Option<Price>) -> Result<Trade>;

    /// Place a new isolated trade.
    ///
    /// **Required permissions**: `futures:isolated:write`
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
    async fn get_funding_fees(&self) -> Result<()>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Cross endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesCrossRepository: crate::sealed::Sealed + Send + Sync {
    /// Cancel all open cross orders.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn cancel_all_orders(&self) -> Result<()>;

    /// Cancel an open cross order.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn cancel_order(&self) -> Result<()>;

    /// Place a new cross order.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn place_order(&self) -> Result<()>;

    /// Get all the cross orders that are still open.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn get_open_orders(&self) -> Result<()>;

    /// Get the current cross margin position.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn get_position(&self) -> Result<()>;

    /// Get the cross orders that have been filled.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn get_filled_orders(&self) -> Result<()>;

    /// Close the running cross margin position. This will pass a market order opposite to the
    /// current position.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn close_position(&self) -> Result<()>;

    /// Get the funding fees paid for the cross margin position.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn get_funding_fees(&self) -> Result<()>;

    /// Get the transfers history for the cross margin position (deposits to and withdrawals from
    /// the cross margin account). Positive amounts are deposits, negative amounts are withdrawals.
    ///
    /// **Required permissions**: `futures:cross:read`
    async fn get_transfers(&self) -> Result<()>;

    /// Deposit funds to the cross margin account.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn deposit(&self) -> Result<()>;

    /// Set the leverage of the cross margin position. If the available margin is not enough to
    /// cover the new position, some of the PL will be realized to cover the difference if possible.
    /// Returns the updated position.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn set_leverage(&self) -> Result<()>;

    /// Withdraw funds from the cross margin account.
    ///
    /// **Required permissions**: `futures:cross:write`
    async fn withdraw(&self) -> Result<()>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Futures Data endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesDataRepository: crate::sealed::Sealed + Send + Sync {
    /// Get the funding settlement history. A settlement happens every 8 hours (00:00, 08:00,
    /// 16:00 UTC).
    async fn get_funding_settlements(&self) -> Result<()>;

    /// Get the futures ticker. [LNM docs].
    ///
    /// [LNM docs]: https://api.lnmarkets.com/v3#tag/futures-data/get/futures/ticker
    async fn get_ticker(&self) -> Result<Ticker>;

    /// Get the candles (OHLCs) history for a given range.
    async fn get_candles(&self) -> Result<()>;

    /// Get the 10 first users by P&L, broken down by day/week/month/all-time.
    async fn get_leaderboard(&self) -> Result<()>;
}

/// Methods for interacting with [LNM's v3 API]'s REST Synthetic USD endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
pub trait SyntheticUsdRepository: crate::sealed::Sealed + Send + Sync {
    /// Fetch the user's swaps.
    ///
    /// **Required permissions**: `synthetic-usd:read`
    async fn get_swaps(&self) -> Result<()>;

    /// Create a new swap.
    ///
    /// **Required permissions**: `synthetic-usd:write`
    async fn create_new_swap(&self) -> Result<()>;

    /// Get best price.
    async fn get_best_price(&self) -> Result<()>;
}
