use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::num::NonZeroU64;
use uuid::Uuid;

use super::{
    error::Result,
    models::{
        Leverage, LnmTrade, Price, PriceEntryLNM, Ticker, TradeExecution, TradeSide, TradeSize,
        TradeStatus, User,
    },
};

#[async_trait]
pub trait FuturesRepository: Send + Sync {
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    async fn get_trades_open(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    async fn get_trades_running(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    async fn get_trades_closed(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    async fn price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>>;

    async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<LnmTrade>;

    async fn get_trade(&self, id: Uuid) -> Result<LnmTrade>;

    async fn cancel_trade(&self, id: Uuid) -> Result<LnmTrade>;

    async fn cancel_all_trades(&self) -> Result<Vec<LnmTrade>>;

    async fn close_trade(&self, id: Uuid) -> Result<LnmTrade>;

    async fn close_all_trades(&self) -> Result<Vec<LnmTrade>>;

    async fn ticker(&self) -> Result<Ticker>;

    async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> Result<LnmTrade>;

    async fn update_trade_takeprofit(&self, id: Uuid, takeprofit: Price) -> Result<LnmTrade>;

    /// Adds margin to a trade, increasing the collateral.
    ///
    /// The resulting `Leverage` (`Quantity` * 100000000 / (`Margin` * `Price`))
    /// must be valid (≥ 1) after the update.
    /// Beware of potential rounding issues when evaluating the new leverage.
    async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade>;

    /// Removes funds from a trade, decreasing the collateral.
    ///
    /// Funds are first removed from the trade's PL (if any), then from the trade's margin.
    /// The resulting `Leverage` (`Quantity` * 100000000 / (`Margin` * `Price`))
    /// must be valid (≥ 1 and ≤ 100) after the update.
    /// Beware of potential rounding issues when evaluating the new leverage.
    async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade>;
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn get_user(&self) -> Result<User>;
}
