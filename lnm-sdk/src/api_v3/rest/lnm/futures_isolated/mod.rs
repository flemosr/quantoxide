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
        trade::{TradeExecution, TradeSide, TradeSize},
    },
    rest::{error::Result, lnm::base::LnmRestBase},
};

use super::{
    super::{
        error::RestApiV3Error,
        models::trade::{FuturesIsolatedTradeRequestBody, Trade},
        repositories::FuturesIsolatedRepository,
    },
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmFuturesIsolatedRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmFuturesIsolatedRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmFuturesIsolatedRepository {}

#[async_trait]
impl FuturesIsolatedRepository for LnmFuturesIsolatedRepository {
    async fn add_margin_to_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade> {
        todo!()
    }

    async fn cancel_all_trades(&self) -> Result<Vec<Trade>> {
        self.base
            .make_request_without_params(
                Method::POST,
                RestPathV3::FuturesIsolatedTradesCancelAll,
                true,
            )
            .await
    }

    async fn cancel_trade(&self, id: Uuid) -> Result<Trade> {
        let body = json!({"id": id.to_string()});

        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesIsolatedTradeCancel,
                body,
                true,
            )
            .await
    }

    async fn cash_in_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade> {
        todo!()
    }

    async fn close_trade(&self, id: Uuid) -> Result<Trade> {
        let body = json!({"id": id.to_string()});

        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesIsolatedTradeClose,
                body,
                true,
            )
            .await
    }

    async fn get_open_trades(&self) -> Result<Vec<Trade>> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesIsolatedTradesOpen, true)
            .await
    }

    async fn get_running_trades(&self) -> Result<Vec<Trade>> {
        self.base
            .make_request_without_params(
                Method::GET,
                RestPathV3::FuturesIsolatedTradesRunning,
                true,
            )
            .await
    }

    async fn get_closed_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
    ) -> Result<Vec<Trade>> {
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
                RestPathV3::FuturesIsolatedTradesClosed,
                query_params,
                true,
            )
            .await
    }

    async fn get_canceled_trades(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
    ) -> Result<Vec<Trade>> {
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
                RestPathV3::FuturesIsolatedTradesCanceled,
                query_params,
                true,
            )
            .await
    }

    async fn update_takeprofit(&self, id: Uuid, value: u64) -> Result<Trade> {
        todo!()
    }

    async fn update_stoploss(&self, id: Uuid, value: u64) -> Result<Trade> {
        todo!()
    }

    async fn new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        client_id: Option<String>,
    ) -> Result<Trade> {
        let body = FuturesIsolatedTradeRequestBody::new(
            leverage, stoploss, takeprofit, side, client_id, size, execution,
        )
        .map_err(RestApiV3Error::FuturesIsolatedTradeRequestValidation)?;

        self.base
            .make_request_with_body(Method::POST, RestPathV3::FuturesIsolatedTrade, body, true)
            .await
    }
}

#[cfg(test)]
mod tests;
