use std::sync::Arc;

use crate::shared::{
    config::RestClientConfig,
    rest::{error::Result, lnm::base::LnmRestBase},
};

pub(crate) mod error;
mod lnm;
pub(super) mod models;
pub(super) mod repositories;

use lnm::{futures_isolated::LnmFuturesIsolatedRepository, signature::SignatureGeneratorV3};
use repositories::FuturesIsolatedRepository;

/// Client for interacting with the [LNM's v3 API] via REST.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3
pub struct RestClient {
    /// Will be `true` if LNM credentials were provided, and `false` otherwise.
    /// See [`ApiClient::with_credentials`].
    ///
    /// [`ApiClient::with_credentials`]: crate::ApiClient::with_credentials
    pub has_credentials: bool,

    /// Methods for interacting with [LNM's v3 API]'s REST Futures endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub futures_isolated: Box<dyn FuturesIsolatedRepository>,
}

impl RestClient {
    fn new_inner(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        let has_credentials = base.has_credentials();
        let futures_isolated = Box::new(LnmFuturesIsolatedRepository::new(base.clone()));

        Self {
            has_credentials,
            futures_isolated,
        }
    }

    pub(in crate::api_v3) fn new(
        config: impl Into<RestClientConfig>,
        domain: String,
    ) -> Result<Self> {
        let base = LnmRestBase::new(config.into(), domain)?;

        Ok(Self::new_inner(base))
    }

    pub(in crate::api_v3) fn with_credentials(
        config: impl Into<RestClientConfig>,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Self> {
        let base = LnmRestBase::with_credentials(
            config.into(),
            domain,
            key,
            passphrase,
            SignatureGeneratorV3::new(secret),
        )?;

        Ok(Self::new_inner(base))
    }
}
