use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::rest::models::{
    Leverage, NestedTradesResponse, Price, Ticker, TradeExecution, TradeSize, TradeStatus,
};

use super::super::{
    error::{RestApiError, Result},
    models::{FuturesTradeRequestBody, PriceEntryLNM, Trade, TradeSide},
    repositories::FuturesRepository,
};
use super::base::LnmApiBase;

#[cfg(test)]
mod tests;

const PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";
const FUTURES_TRADE_PATH: &str = "/v2/futures";
const FUTURES_TICKER_PATH: &str = "/v2/futures/ticker";
const FUTURES_CANCEL_TRADE_PATH: &str = "/v2/futures/cancel";
const FUTURES_CANCEL_ALL_TRADES_PATH: &str = "/v2/futures/all/cancel";

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
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        let mut query_params = Vec::new();

        query_params.push(("type", status.to_string()));

        if let Some(from) = from {
            query_params.push(("from", from.timestamp_millis().to_string()));
        }
        if let Some(to) = to {
            query_params.push(("to", to.timestamp_millis().to_string()));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let trades: Vec<Trade> = self
            .base
            .make_request_with_query_params(Method::GET, FUTURES_TRADE_PATH, query_params, true)
            .await?;

        Ok(trades)
    }

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

    async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade> {
        let body =
            FuturesTradeRequestBody::new(leverage, stoploss, takeprofit, side, size, execution)
                .map_err(|e| RestApiError::Generic(e.to_string()))?;

        let created_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, FUTURES_TRADE_PATH, body, true)
            .await?;

        Ok(created_trade)
    }

    async fn cancel_trade(&self, id: Uuid) -> Result<Trade> {
        let body = json!({"id": id.to_string()});

        let canceled_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, FUTURES_CANCEL_TRADE_PATH, body, true)
            .await?;

        Ok(canceled_trade)
    }

    async fn cancel_all_trades(&self) -> Result<Vec<Trade>> {
        let res: NestedTradesResponse = self
            .base
            .make_request_without_params(Method::DELETE, FUTURES_CANCEL_ALL_TRADES_PATH, true)
            .await?;

        Ok(res.trades)
    }

    async fn close_trade(&self, id: Uuid) -> Result<Trade> {
        let query_params = vec![("id", id.to_string())];

        let deleted_trade: Trade = self
            .base
            .make_request_with_query_params(Method::DELETE, FUTURES_TRADE_PATH, query_params, true)
            .await?;

        Ok(deleted_trade)
    }

    async fn ticker(&self) -> Result<Ticker> {
        let ticker: Ticker = self
            .base
            .make_request_without_params(Method::GET, FUTURES_TICKER_PATH, true)
            .await?;

        Ok(ticker)
    }
}
