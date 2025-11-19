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

    async fn get_last_unused_onchain_address(&self) -> Result<()> {
        todo!()
    }

    async fn generate_new_bitcoin_address(&self) -> Result<()> {
        todo!()
    }

    async fn get_notifications(&self) -> Result<()> {
        todo!()
    }

    async fn mark_notifications_read(&self) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
