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
/// Some endpoints require credentials with specific permissions. Such requirements will be
/// mentioned in the corresponding method's documentation".
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3
pub struct RestClient {
    /// Indicates whether LNM credentials were provided during client initialization.
    ///
    /// Will be `true` if the client was created with [`RestClient::with_credentials`],
    /// and `false` if created with [`RestClient::new`].
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
    fn new_inner(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Arc<Self> {
        let has_credentials = base.has_credentials();
        let utilities = Box::new(LnmUtilitiesRepository::new(base.clone()));
        let futures_isolated = Box::new(LnmFuturesIsolatedRepository::new(base.clone()));
        let futures_cross = Box::new(LnmFuturesCrossRepository::new(base.clone()));
        let futures_data = Box::new(LnmFuturesDataRepository::new(base.clone()));
        let account = Box::new(LnmAccountRepository::new(base.clone()));
        let oracle = Box::new(LnmOracleRepository::new(base));

        Arc::new(Self {
            has_credentials,
            utilities,
            futures_isolated,
            futures_cross,
            futures_data,
            account,
            oracle,
        })
    }

    /// Creates a new unauthenticated REST client.
    ///
    /// For authenticated endpoints, use [`RestClient::with_credentials`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v3::{RestClient, RestClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    ///
    /// let client = RestClient::new(RestClientConfig::default(), domain)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: impl Into<RestClientConfig>, domain: impl ToString) -> Result<Arc<Self>> {
        let base = LnmRestBase::new(config.into(), domain.to_string())?;

        Ok(Self::new_inner(base))
    }

    /// Creates a new authenticated REST client with credentials.
    ///
    /// If not accessing authenticated endpoints, consider using [`RestClient::new`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v3::{RestClient, RestClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let key = env::var("LNM_API_KEY").unwrap();
    /// let secret = env::var("LNM_API_SECRET").unwrap();
    /// let pphrase = env::var("LNM_API_PASSPHRASE").unwrap();
    ///
    /// let config = RestClientConfig::default();
    /// let client = RestClient::with_credentials(config, domain, key, secret, pphrase)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_credentials(
        config: impl Into<RestClientConfig>,
        domain: impl ToString,
        key: impl ToString,
        secret: impl ToString,
        passphrase: impl ToString,
    ) -> Result<Arc<Self>> {
        let base = LnmRestBase::with_credentials(
            config.into(),
            domain.to_string(),
            key.to_string(),
            passphrase.to_string(),
            SignatureGeneratorV3::new(secret.to_string()),
        )?;

        Ok(Self::new_inner(base))
    }
}
