use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::{
    error::Result,
    models::{Leverage, Margin, Price, PriceEntryLNM, Quantity, Trade, TradeSide},
};

#[async_trait]
pub trait FuturesRepository: Send + Sync {
    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>>;

    async fn create_new_trade_quantity_limit(
        &self,
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        price: Price,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade>;

    async fn create_new_trade_margin_limit(
        &self,
        side: TradeSide,
        margin: Margin,
        leverage: Leverage,
        price: Price,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade>;
}
