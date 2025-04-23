use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::{
    error::Result,
    models::{
        Leverage, Price, PriceEntryLNM, Trade, TradeExecution, TradeSide, TradeSize, TradeStatus,
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
}
