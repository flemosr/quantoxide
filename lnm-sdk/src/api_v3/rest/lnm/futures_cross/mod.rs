use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use hyper::Method;
use serde_json::json;
use uuid::Uuid;

use crate::shared::{
    models::{
        quantity::Quantity,
        trade::{TradeExecution, TradeSide},
    },
    rest::{error::Result, lnm::base::LnmRestBase},
};

use super::{
    super::{
        error::RestApiV3Error,
        models::{
            cross_leverage::CrossLeverage,
            funding::PaginatedFundingSettlements,
            trade::{CrossOrder, CrossPosition, FuturesCrossOrderBody, PaginatedCrossOrders},
            transfer::PaginatedCrossTransfers,
        },
        repositories::FuturesCrossRepository,
    },
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmFuturesCrossRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmFuturesCrossRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmFuturesCrossRepository {}

#[async_trait]
impl FuturesCrossRepository for LnmFuturesCrossRepository {
    async fn cancel_all_orders(&self) -> Result<Vec<CrossOrder>> {
        self.base
            .make_request_without_params(
                Method::POST,
                RestPathV3::FuturesCrossOrdersCancelAll,
                true,
            )
            .await
    }

    async fn cancel_order(&self, id: Uuid) -> Result<CrossOrder> {
        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesCrossOrderCancel,
                json!({ "id": id }),
                true,
            )
            .await
    }

    async fn place_order(
        &self,
        side: TradeSide,
        quantity: Quantity,
        execution: TradeExecution,
        client_id: Option<String>,
    ) -> Result<CrossOrder> {
        let body = FuturesCrossOrderBody::new(side, quantity, execution, client_id)
            .map_err(RestApiV3Error::FuturesCrossTradeOrderValidation)?;

        self.base
            .make_request_with_body(Method::POST, RestPathV3::FuturesCrossOrder, body, true)
            .await
    }

    async fn get_open_orders(&self) -> Result<Vec<CrossOrder>> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesCrossOrdersOpen, true)
            .await
    }

    async fn get_position(&self) -> Result<CrossPosition> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesCrossPosition, true)
            .await
    }

    async fn get_filled_orders(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<PaginatedCrossOrders> {
        let mut query_params = Vec::new();

        if let Some(from) = from {
            query_params.push(("from", from.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(to) = to {
            query_params.push(("to", to.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = cursor {
            query_params.push((
                "cursor",
                cursor.to_rfc3339_opts(SecondsFormat::Millis, true),
            ));
        }

        self.base
            .make_request_with_query_params(
                Method::GET,
                RestPathV3::FuturesCrossOrdersFilled,
                query_params,
                true,
            )
            .await
    }

    async fn close_position(&self) -> Result<CrossOrder> {
        self.base
            .make_request_without_params(Method::POST, RestPathV3::FuturesCrossPositionClose, true)
            .await
    }

    async fn get_funding_fees(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<PaginatedFundingSettlements> {
        let mut query_params = Vec::new();

        if let Some(from) = from {
            query_params.push(("from", from.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(to) = to {
            query_params.push(("to", to.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = cursor {
            query_params.push((
                "cursor",
                cursor.to_rfc3339_opts(SecondsFormat::Millis, true),
            ));
        }

        self.base
            .make_request_with_query_params(
                Method::GET,
                RestPathV3::FuturesCrossFundingFees,
                query_params,
                true,
            )
            .await
    }

    async fn get_transfers(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<PaginatedCrossTransfers> {
        let mut query_params = Vec::new();

        if let Some(from) = from {
            query_params.push(("from", from.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(to) = to {
            query_params.push(("to", to.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = cursor {
            query_params.push((
                "cursor",
                cursor.to_rfc3339_opts(SecondsFormat::Millis, true),
            ));
        }

        self.base
            .make_request_with_query_params(
                Method::GET,
                RestPathV3::FuturesCrossGetTransfers,
                query_params,
                true,
            )
            .await
    }

    async fn deposit(&self, amount: NonZeroU64) -> Result<CrossPosition> {
        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesCrossDeposit,
                json!({ "amount": amount }),
                true,
            )
            .await
    }

    async fn set_leverage(&self, leverage: CrossLeverage) -> Result<CrossPosition> {
        self.base
            .make_request_with_body(
                Method::PUT,
                RestPathV3::FuturesCrossPositionSetLeverage,
                json!({ "leverage": leverage }),
                true,
            )
            .await
    }

    async fn withdraw(&self, amount: NonZeroU64) -> Result<CrossPosition> {
        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesCrossWithdraw,
                json!({ "amount": amount }),
                true,
            )
            .await
    }
}

#[cfg(test)]
mod tests;
