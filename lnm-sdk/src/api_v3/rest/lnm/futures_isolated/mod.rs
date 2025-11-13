use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{self, Method};

use crate::{
    api_v3::rest::models::trade::FuturesIsolatedTradeRequestBody,
    shared::{
        models::{
            leverage::Leverage,
            price::Price,
            trade::{TradeExecution, TradeSide, TradeSize},
        },
        rest::{error::Result, lnm::base::LnmRestBase},
    },
};

use super::{
    super::{error::RestApiV3Error, models::trade::Trade, repositories::FuturesIsolatedRepository},
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
