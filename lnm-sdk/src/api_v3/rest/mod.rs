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
    account::LnmAccountRepository, futures_cross::LnmFuturesCrossRepository,
    futures_data::LnmFuturesDataRepository, futures_isolated::LnmFuturesIsolatedRepository,
    oracle::LnmOracleRepository, signature::SignatureGeneratorV3,
    utilities::LnmUtilitiesRepository,
};
use repositories::{
    AccountRepository, FuturesCrossRepository, FuturesDataRepository, FuturesIsolatedRepository,
    OracleRepository, UtilitiesRepository,
};

/// Client for interacting with the [LNM's v3 API] via REST.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3
pub struct RestClient {
    /// Will be `true` if LNM credentials were provided, and `false` otherwise.
    /// See [`ApiClient::with_credentials`].
    ///
    /// [`ApiClient::with_credentials`]: crate::ApiClient::with_credentials
    pub has_credentials: bool,

    /// Methods for interacting with [LNM's v3 API]'s REST Utilities endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub utilities: Box<dyn UtilitiesRepository>,

    /// Methods for interacting with [LNM's v3 API]'s REST Futures Isolated endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub futures_isolated: Box<dyn FuturesIsolatedRepository>,

    /// Methods for interacting with [LNM's v3 API]'s REST Futures Cross endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub futures_cross: Box<dyn FuturesCrossRepository>,

    /// Methods for interacting with [LNM's v3 API]'s REST Futures Data endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub futures_data: Box<dyn FuturesDataRepository>,

    /// Methods for interacting with [LNM's v3 API]'s REST Account endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub account: Box<dyn AccountRepository>,

    /// Methods for interacting with [LNM's v3 API]'s REST Oracle endpoints.
    ///
    /// [LNM's v3 API]: https://api.lnmarkets.com/v3
    pub oracle: Box<dyn OracleRepository>,
}

impl RestClient {
    fn new_inner(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        let has_credentials = base.has_credentials();
        let utilities = Box::new(LnmUtilitiesRepository::new(base.clone()));
        let futures_isolated = Box::new(LnmFuturesIsolatedRepository::new(base.clone()));
        let futures_cross = Box::new(LnmFuturesCrossRepository::new(base.clone()));
        let futures_data = Box::new(LnmFuturesDataRepository::new(base.clone()));
        let account = Box::new(LnmAccountRepository::new(base.clone()));
        let oracle = Box::new(LnmOracleRepository::new(base));

        Self {
            has_credentials,
            utilities,
            futures_isolated,
            futures_cross,
            futures_data,
            account,
            oracle,
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
