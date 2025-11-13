use async_trait::async_trait;

use crate::shared::{
    models::{
        leverage::Leverage,
        price::Price,
        trade::{TradeExecution, TradeSide, TradeSize},
    },
    rest::error::Result,
};

use super::models::trade::Trade;

/// Methods for interacting with [LNM's v3 API]'s REST Futures endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesIsolatedRepository: crate::sealed::Sealed + Send + Sync {
    /// **Requires credentials**. Place a new isolated trade.
    async fn new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade>;
}
