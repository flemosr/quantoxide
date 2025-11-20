use std::sync::Arc;

use crate::shared::{
    config::RestClientConfig,
    rest::{error::Result, lnm::base::LnmRestBase},
};

pub(crate) mod error;
mod lnm;
pub(super) mod models;
pub(super) mod repositories;

use lnm::{
    futures::LnmFuturesRepository, signature::SignatureGeneratorV2, user::LnmUserRepository,
};
use repositories::{FuturesRepository, UserRepository};

/// Client for interacting with the [LNM's v2 API] via REST.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
pub struct RestClient {
    /// Will be `true` if LNM credentials were provided, and `false` otherwise.
    /// See [`ApiClient::with_credentials`].
    ///
    /// [`ApiClient::with_credentials`]: crate::ApiClient::with_credentials
    pub has_credentials: bool,

    /// Methods for interacting with [LNM's v2 API]'s REST Futures endpoints.
    ///
    /// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
    pub futures: Box<dyn FuturesRepository>,

    /// Methods for interacting with [LNM's v2 API]'s REST User endpoints.
    ///
    /// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
    pub user: Box<dyn UserRepository>,
}

impl RestClient {
    fn new_inner(base: Arc<LnmRestBase<SignatureGeneratorV2>>) -> Arc<Self> {
        let has_credentials = base.has_credentials();
        let futures = Box::new(LnmFuturesRepository::new(base.clone()));
        let user = Box::new(LnmUserRepository::new(base));

        Arc::new(Self {
            has_credentials,
            futures,
            user,
        })
    }

    pub fn new(config: impl Into<RestClientConfig>, domain: String) -> Result<Arc<Self>> {
        let base = LnmRestBase::new(config.into(), domain)?;

        Ok(Self::new_inner(base))
    }

    pub fn with_credentials(
        config: impl Into<RestClientConfig>,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Arc<Self>> {
        let base = LnmRestBase::with_credentials(
            config.into(),
            domain,
            key,
            passphrase,
            SignatureGeneratorV2::new(secret),
        )?;

        Ok(Self::new_inner(base))
    }
}
