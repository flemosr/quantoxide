use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use serde_json::json;
use uuid::Uuid;

use crate::shared::{
    models::{
        leverage::Leverage,
        price::Price,
        trade::{TradeExecution, TradeSide, TradeSize, TradeStatus},
    },
    rest::{error::Result, lnm::base::LnmRestBase},
};

use super::{
    super::{
        error::RestApiV2Error,
        models::{
            price_history::PriceEntry,
            ticker::Ticker,
            trade::{
                FuturesTradeRequestBody, FuturesUpdateTradeRequestBody, NestedTradesResponse,
                Trade, TradeUpdateType,
            },
        },
        repositories::FuturesRepository,
    },
    base::ApiPathV2,
};

pub(in crate::api_v2) struct LnmFuturesRepository {
    base: Arc<LnmRestBase>,
}

impl LnmFuturesRepository {
    pub fn new(base: Arc<LnmRestBase>) -> Self {
        Self { base }
    }

    async fn update_trade(
        &self,
        id: Uuid,
        update_type: TradeUpdateType,
        value: Price,
    ) -> Result<Trade> {
        let body = FuturesUpdateTradeRequestBody::new(id, update_type, value);

        self.base
            .make_request_with_body(Method::PUT, ApiPathV2::FuturesTrade, body, true)
            .await
    }
}

impl crate::sealed::Sealed for LnmFuturesRepository {}

#[async_trait]
impl FuturesRepository for LnmFuturesRepository {
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
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

        self.base
            .make_request_with_query_params(
                Method::GET,
                ApiPathV2::FuturesTrade,
                query_params,
                true,
            )
            .await
    }

    async fn get_trades_open(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        self.get_trades(TradeStatus::Open, from, to, limit).await
    }

    async fn get_trades_running(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        self.get_trades(TradeStatus::Running, from, to, limit).await
    }

    async fn get_trades_closed(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        self.get_trades(TradeStatus::Closed, from, to, limit).await
    }

    async fn price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntry>> {
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
                ApiPathV2::FuturesPriceHistory,
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
    ) -> Result<Trade> {
        let body =
            FuturesTradeRequestBody::new(leverage, stoploss, takeprofit, side, size, execution)
                .map_err(RestApiV2Error::FuturesTradeRequestValidation)?;

        self.base
            .make_request_with_body(Method::POST, ApiPathV2::FuturesTrade, body, true)
            .await
    }

    async fn get_trade(&self, id: Uuid) -> Result<Trade> {
        self.base
            .make_request_without_params(Method::GET, ApiPathV2::FuturesGetTrade(id), true)
            .await
    }

    async fn cancel_trade(&self, id: Uuid) -> Result<Trade> {
        let body = json!({"id": id.to_string()});

        self.base
            .make_request_with_body(Method::POST, ApiPathV2::FuturesCancelTrade, body, true)
            .await
    }

    async fn cancel_all_trades(&self) -> Result<Vec<Trade>> {
        let res: NestedTradesResponse = self
            .base
            .make_request_without_params(Method::DELETE, ApiPathV2::FuturesCancelAllTrades, true)
            .await?;

        Ok(res.trades)
    }

    async fn close_trade(&self, id: Uuid) -> Result<Trade> {
        let query_params = vec![("id", id.to_string())];

        self.base
            .make_request_with_query_params(
                Method::DELETE,
                ApiPathV2::FuturesTrade,
                query_params,
                true,
            )
            .await
    }

    async fn close_all_trades(&self) -> Result<Vec<Trade>> {
        let res: NestedTradesResponse = self
            .base
            .make_request_without_params(Method::DELETE, ApiPathV2::FuturesCloseAllTrades, true)
            .await?;

        Ok(res.trades)
    }

    async fn ticker(&self) -> Result<Ticker> {
        self.base
            .make_request_without_params(Method::GET, ApiPathV2::FuturesTicker, true)
            .await
    }

    async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> Result<Trade> {
        self.update_trade(id, TradeUpdateType::Stoploss, stoploss)
            .await
    }

    async fn update_trade_takeprofit(&self, id: Uuid, takeprofit: Price) -> Result<Trade> {
        self.update_trade(id, TradeUpdateType::Takeprofit, takeprofit)
            .await
    }

    async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade> {
        let body = json!({
            "id": id.to_string(),
            "amount": amount,
        });

        self.base
            .make_request_with_body(Method::POST, ApiPathV2::FuturesAddMargin, body, true)
            .await
    }

    async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade> {
        let body = json!({
            "id": id.to_string(),
            "amount": amount,
        });

        self.base
            .make_request_with_body(Method::POST, ApiPathV2::FuturesCashIn, body, true)
            .await
    }
}

#[cfg(test)]
mod tests;
