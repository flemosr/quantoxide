use std::sync::Arc;

use async_trait::async_trait;
use hyper::Method;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api_v3::models::CrossLeverage,
    shared::{
        models::{
            quantity::Quantity,
            trade::{TradeExecution, TradeSide},
        },
        rest::{error::Result, lnm::base::LnmRestBase},
    },
};

use super::{
    super::{
        error::RestApiV3Error,
        models::trade::{CrossOrder, CrossPosition, FuturesCrossOrderBody},
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
            .make_request_without_params(Method::POST, RestPathV3::FuturesCrossOrderCancelAll, true)
            .await
    }

    async fn cancel_order(&self, id: Uuid) -> Result<CrossOrder> {
        self.base
            .make_request_with_body(
                Method::POST,
                RestPathV3::FuturesCrossOrderCancel,
                json!({"id": id}),
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

    async fn get_open_orders(&self) -> Result<()> {
        todo!()
    }

    async fn get_position(&self) -> Result<CrossPosition> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesCrossPosition, true)
            .await
    }

    async fn get_filled_orders(&self) -> Result<()> {
        todo!()
    }

    async fn close_position(&self) -> Result<CrossOrder> {
        self.base
            .make_request_without_params(Method::POST, RestPathV3::FuturesCrossPositionClose, true)
            .await
    }

    async fn get_funding_fees(&self) -> Result<()> {
        todo!()
    }

    async fn get_transfers(&self) -> Result<()> {
        todo!()
    }

    async fn deposit(&self) -> Result<()> {
        todo!()
    }

    async fn set_leverage(&self, leverage: CrossLeverage) -> Result<CrossPosition> {
        self.base
            .make_request_with_body(
                Method::PUT,
                RestPathV3::FuturesCrossPositionSetLeverage,
                json!({"leverage": leverage}),
                true,
            )
            .await
    }

    async fn withdraw(&self) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
