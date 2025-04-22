use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::{
    error::Result,
    models::{FuturePrice, Leverage, Margin, PriceEntryLNM, Trade, TradeSide},
};

#[async_trait]
pub trait FuturesRepository: Send + Sync {
    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>>;

    async fn create_new_trade_margin_limit(
        &self,
        side: TradeSide,
        margin: Margin,
        leverage: Leverage,
        price: FuturePrice,
        stoploss: Option<FuturePrice>,
        takeprofit: Option<FuturePrice>,
    ) -> Result<Trade>;
}
