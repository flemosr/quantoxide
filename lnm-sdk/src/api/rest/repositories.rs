use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{
    error::Result,
    models::{
        Leverage, Margin, Price, PriceEntryLNM, Ticker, Trade, TradeExecution, TradeSide,
        TradeSize, TradeStatus,
    },
};

#[async_trait]
pub trait FuturesRepository: Send + Sync {
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>>;

    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
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
    ) -> Result<Trade>;

    async fn cancel_trade(&self, id: Uuid) -> Result<Trade>;

    async fn cancel_all_trades(&self) -> Result<Vec<Trade>>;

    async fn close_trade(&self, id: Uuid) -> Result<Trade>;

    async fn close_all_trades(&self) -> Result<Vec<Trade>>;

    async fn ticker(&self) -> Result<Ticker>;

    async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> Result<Trade>;

    async fn update_trade_takeprofit(&self, id: Uuid, takeprofit: Price) -> Result<Trade>;

    /// Adds margin to a trade, increasing the collateral.
    ///
    /// The resulting `Leverage` (`Quantity` * 100000000 / (`Margin` * `Price`))
    /// must be valid (≥ 1) after the update.
    /// Beware of potential rounding issues when evaluating the new leverage.
    async fn add_margin(&self, id: Uuid, margin: Margin) -> Result<Trade>;
}
