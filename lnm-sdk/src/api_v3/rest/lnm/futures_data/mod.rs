use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{self, Method};

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{models::ticker::Ticker, repositories::FuturesDataRepository},
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmFuturesDataRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmFuturesDataRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmFuturesDataRepository {}

#[async_trait]
impl FuturesDataRepository for LnmFuturesDataRepository {
    async fn get_ticker(&self) -> Result<Ticker> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesDataTicker, false)
            .await
    }
}

#[cfg(test)]
mod tests;
