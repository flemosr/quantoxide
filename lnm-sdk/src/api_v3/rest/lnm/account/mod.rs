use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{models::account::Account, repositories::AccountRepository},
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmAccountRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmAccountRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmAccountRepository {}

#[async_trait]
impl AccountRepository for LnmAccountRepository {
    async fn get_account(&self) -> Result<Account> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::Account, true)
            .await
    }
}

#[cfg(test)]
mod tests;
