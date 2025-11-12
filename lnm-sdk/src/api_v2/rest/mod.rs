use std::{sync::Arc, time::Duration};

pub(crate) mod error;
mod lnm;
pub(crate) mod models;
pub(crate) mod repositories;

use error::Result;
use lnm::{base::LnmRestBase, futures::LnmFuturesRepository, user::LnmUserRepository};
use repositories::{FuturesRepository, UserRepository};

use super::client::ApiClientConfig;

#[derive(Clone, Debug)]
pub(crate) struct RestClientConfig {
    timeout: Duration,
}

impl From<&ApiClientConfig> for RestClientConfig {
    fn from(value: &ApiClientConfig) -> Self {
        Self {
            timeout: value.rest_timeout(),
        }
    }
}

impl Default for RestClientConfig {
    fn default() -> Self {
        (&ApiClientConfig::default()).into()
    }
}

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
    fn new_inner(base: Arc<LnmRestBase>) -> Self {
        let has_credentials = base.has_credentials();
        let futures = Box::new(LnmFuturesRepository::new(base.clone()));
        let user = Box::new(LnmUserRepository::new(base));

        Self {
            has_credentials,
            futures,
            user,
        }
    }

    pub(crate) fn new(config: impl Into<RestClientConfig>, domain: String) -> Result<Self> {
        let base = LnmRestBase::new(config.into(), domain)?;

        Ok(Self::new_inner(base))
    }

    pub(crate) fn with_credentials(
        config: impl Into<RestClientConfig>,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Self> {
        let base = LnmRestBase::with_credentials(config.into(), domain, key, secret, passphrase)?;

        Ok(Self::new_inner(base))
    }
}
