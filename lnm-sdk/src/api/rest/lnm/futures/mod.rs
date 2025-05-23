use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use serde_json::json;
use std::{num::NonZeroU64, sync::Arc};
use uuid::Uuid;

use crate::api::rest::models::{
    FuturesUpdateTradeRequestBody, Leverage, NestedTradesResponse, Price, Ticker, TradeExecution,
    TradeSize, TradeStatus, TradeUpdateType,
};

use super::{
    super::{
        error::{RestApiError, Result},
        models::{FuturesTradeRequestBody, LnmTrade, PriceEntryLNM, TradeSide},
        repositories::FuturesRepository,
    },
    base::{ApiPath, LnmApiBase},
};

#[cfg(test)]
mod tests;

pub struct LnmFuturesRepository {
    base: Arc<LnmApiBase>,
}

impl LnmFuturesRepository {
    pub fn new(base: Arc<LnmApiBase>) -> Self {
        Self { base }
    }

    async fn update_trade(
        &self,
        id: Uuid,
        update_type: TradeUpdateType,
        value: Price,
    ) -> Result<LnmTrade> {
        let body = FuturesUpdateTradeRequestBody::new(id, update_type, value);

        self.base
            .make_request_with_body(Method::PUT, ApiPath::FuturesTrade, body, true)
            .await
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
    ) -> Result<Vec<LnmTrade>> {
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

        self.base
            .make_request_with_query_params(Method::GET, ApiPath::FuturesTrade, query_params, true)
            .await
    }

    async fn get_trades_open(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>> {
        self.get_trades(TradeStatus::Open, from, to, limit).await
    }

    async fn get_trades_running(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>> {
        self.get_trades(TradeStatus::Running, from, to, limit).await
    }

    async fn get_trades_closed(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>> {
        self.get_trades(TradeStatus::Closed, from, to, limit).await
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

        self.base
            .make_request_with_query_params(
                Method::GET,
                ApiPath::FuturesPriceHistory,
                query_params,
                false,
            )
            .await
    }

    async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<LnmTrade> {
        let body =
            FuturesTradeRequestBody::new(leverage, stoploss, takeprofit, side, size, execution)
                .map_err(|e| RestApiError::Generic(e.to_string()))?;

        self.base
            .make_request_with_body(Method::POST, ApiPath::FuturesTrade, body, true)
            .await
    }

    async fn get_trade(&self, id: Uuid) -> Result<LnmTrade> {
        self.base
            .make_request_without_params(Method::GET, ApiPath::FuturesGetTrade(id), true)
            .await
    }

    async fn cancel_trade(&self, id: Uuid) -> Result<LnmTrade> {
        let body = json!({"id": id.to_string()});

        self.base
            .make_request_with_body(Method::POST, ApiPath::FuturesCancelTrade, body, true)
            .await
    }

    async fn cancel_all_trades(&self) -> Result<Vec<LnmTrade>> {
        let res: NestedTradesResponse = self
            .base
            .make_request_without_params(Method::DELETE, ApiPath::FuturesCancelAllTrades, true)
            .await?;

        Ok(res.trades)
    }

    async fn close_trade(&self, id: Uuid) -> Result<LnmTrade> {
        let query_params = vec![("id", id.to_string())];

        self.base
            .make_request_with_query_params(
                Method::DELETE,
                ApiPath::FuturesTrade,
                query_params,
                true,
            )
            .await
    }

    async fn close_all_trades(&self) -> Result<Vec<LnmTrade>> {
        let res: NestedTradesResponse = self
            .base
            .make_request_without_params(Method::DELETE, ApiPath::FuturesCloseAllTrades, true)
            .await?;

        Ok(res.trades)
    }

    async fn ticker(&self) -> Result<Ticker> {
        self.base
            .make_request_without_params(Method::GET, ApiPath::FuturesTicker, true)
            .await
    }

    async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> Result<LnmTrade> {
        self.update_trade(id, TradeUpdateType::Stoploss, stoploss)
            .await
    }

    async fn update_trade_takeprofit(&self, id: Uuid, takeprofit: Price) -> Result<LnmTrade> {
        self.update_trade(id, TradeUpdateType::Takeprofit, takeprofit)
            .await
    }

    async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade> {
        let body = json!({
            "id": id.to_string(),
            "amount": amount,
        });

        self.base
            .make_request_with_body(Method::POST, ApiPath::FuturesAddMargin, body, true)
            .await
    }

    async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade> {
        let body = json!({
            "id": id.to_string(),
            "amount": amount,
        });

        self.base
            .make_request_with_body(Method::POST, ApiPath::FuturesCashIn, body, true)
            .await
    }
}
