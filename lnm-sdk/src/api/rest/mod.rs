use std::{sync::Arc, time::Duration};

pub mod error;
mod lnm;
pub mod models;
mod repositories;

use error::Result;
use lnm::{base::LnmApiBase, futures::LnmFuturesRepository, user::LnmUserRepository};
use repositories::{FuturesRepository, UserRepository};

use super::ApiContextConfig;

#[derive(Clone, Debug)]
pub struct RestApiContextConfig {
    timeout: Duration,
}

impl From<&ApiContextConfig> for RestApiContextConfig {
    fn from(value: &ApiContextConfig) -> Self {
        Self {
            timeout: value.rest_timeout,
        }
    }
}

impl Default for RestApiContextConfig {
    fn default() -> Self {
        (&ApiContextConfig::default()).into()
    }
}

pub struct RestApiContext {
    pub has_credentials: bool,
    pub futures: Box<dyn FuturesRepository>,
    pub user: Box<dyn UserRepository>,
}

impl RestApiContext {
    fn new_inner(base: Arc<LnmApiBase>) -> Self {
        let has_credentials = base.has_credentials();
        let futures = Box::new(LnmFuturesRepository::new(base.clone()));
        let user = Box::new(LnmUserRepository::new(base));

        Self {
            has_credentials,
            futures,
            user,
        }
    }

    pub fn new(config: impl Into<RestApiContextConfig>, domain: String) -> Result<Self> {
        let base = LnmApiBase::new(config.into(), domain)?;

        Ok(Self::new_inner(base))
    }

    pub fn with_credentials(
        config: impl Into<RestApiContextConfig>,
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Self> {
        let base = LnmApiBase::with_credentials(config.into(), domain, key, secret, passphrase)?;

        Ok(Self::new_inner(base))
    }
}
