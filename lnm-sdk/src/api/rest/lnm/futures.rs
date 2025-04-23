use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use std::sync::Arc;

use crate::api::rest::models::{Leverage, Margin, Price, Quantity};

use super::super::{
    error::{RestApiError, Result},
    models::{FuturesTradeRequestBody, PriceEntryLNM, Trade, TradeSide, TradeType},
    repositories::FuturesRepository,
};
use super::base::LnmApiBase;

const PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";
const CREATE_NEW_TRADE_PATH: &str = "/v2/futures";

pub struct LnmFuturesRepository {
    base: Arc<LnmApiBase>,
}

impl LnmFuturesRepository {
    pub fn new(base: Arc<LnmApiBase>) -> Self {
        Self { base }
    }
}

#[async_trait]
impl FuturesRepository for LnmFuturesRepository {
    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>> {
        let mut query_params = Vec::new();
        if let Some(from) = from {
            query_params.push(("from", from.timestamp_millis().to_string()));
        }
        if let Some(to) = to {
            query_params.push(("to", to.timestamp_millis().to_string()));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let price_history: Vec<PriceEntryLNM> = self
            .base
            .make_request_with_query_params(Method::GET, PRICE_HISTORY_PATH, query_params, false)
            .await?;

        Ok(price_history)
    }

    async fn create_new_trade_quantity_limit(
        &self,
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        price: Price,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade> {
        let body = FuturesTradeRequestBody::new(
            leverage,
            stoploss,
            takeprofit,
            side,
            Some(quantity),
            None,
            TradeType::Limit,
            Some(price),
        )
        .map_err(|e| RestApiError::Generic(e.to_string()))?;

        let created_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, CREATE_NEW_TRADE_PATH, Some(body), true)
            .await?;

        Ok(created_trade)
    }

    async fn create_new_trade_margin_limit(
        &self,
        side: TradeSide,
        margin: Margin,
        leverage: Leverage,
        price: Price,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade> {
        let body = FuturesTradeRequestBody::new(
            leverage,
            stoploss,
            takeprofit,
            side,
            None,
            Some(margin),
            TradeType::Limit,
            Some(price),
        )
        .map_err(|e| RestApiError::Generic(e.to_string()))?;

        let created_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, CREATE_NEW_TRADE_PATH, Some(body), true)
            .await?;

        Ok(created_trade)
    }
}
