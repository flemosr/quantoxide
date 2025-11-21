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
/// When credentials are provided, it supports authenticated endpoints.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
pub struct RestClient {
    /// Will be `true` if LNM credentials were provided, and `false` otherwise.
    /// See [`RestClient::with_credentials`].
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

    /// Creates a new unauthenticated REST client.
    ///
    /// For authenticated endpoints, use [`RestClient::with_credentials`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v2::rest::{RestClient, RestClientConfig};
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
    /// use lnm_sdk::api_v2::rest::{RestClient, RestClientConfig};
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
            SignatureGeneratorV2::new(secret.to_string()),
        )?;

        Ok(Self::new_inner(base))
    }
}
