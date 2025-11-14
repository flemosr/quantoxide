use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use reqwest::{self, Method};
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
        todo!()
    }

    async fn cash_in_trade(&self, id: Uuid, amount: NonZeroU64) -> Result<Trade> {
        todo!()
    }

    async fn close_trade(&self, id: Uuid) -> Result<Trade> {
        todo!()
    }

    async fn get_open_trades(&self) -> Result<Vec<Trade>> {
        todo!()
    }

    async fn get_running_trades(&self) -> Result<Vec<Trade>> {
        todo!()
    }

    async fn get_closed_trades(&self) -> Result<Vec<Trade>> {
        todo!()
    }

    async fn get_canceled_trades(&self) -> Result<Vec<Trade>> {
        todo!()
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
