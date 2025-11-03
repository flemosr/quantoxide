use std::{sync::Arc, time::Duration};

pub(crate) mod error;
mod lnm;
pub(crate) mod models;
mod repositories;

use error::Result;
use lnm::{base::LnmRestBase, futures::LnmFuturesRepository, user::LnmUserRepository};
use repositories::{FuturesRepository, UserRepository};

use super::ApiContextConfig;

#[derive(Clone, Debug)]
pub(crate) struct RestClientConfig {
    timeout: Duration,
}

impl From<&ApiContextConfig> for RestClientConfig {
    fn from(value: &ApiContextConfig) -> Self {
        Self {
            timeout: value.rest_timeout,
        }
    }
}

impl Default for RestClientConfig {
    fn default() -> Self {
        (&ApiContextConfig::default()).into()
    }
}

pub struct RestClient {
    pub has_credentials: bool,
    pub futures: Box<dyn FuturesRepository>,
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
