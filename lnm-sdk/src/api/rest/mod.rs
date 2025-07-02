pub mod error;
mod lnm;
pub mod models;
mod repositories;

use error::Result;
use lnm::{base::LnmApiBase, futures::LnmFuturesRepository, user::LnmUserRepository};
use repositories::{FuturesRepository, UserRepository};

pub struct RestApiContext {
    pub futures: Box<dyn FuturesRepository>,
    pub user: Box<dyn UserRepository>,
}

impl RestApiContext {
    pub fn new(domain: String) -> Result<Self> {
        let base = LnmApiBase::new(domain)?;

        let futures = Box::new(LnmFuturesRepository::new(base.clone()));
        let user = Box::new(LnmUserRepository::new(base));

        Ok(Self { futures, user })
    }

    pub fn with_credentials(
        domain: String,
        key: String,
        secret: String,
        passphrase: String,
    ) -> Result<Self> {
        let base = LnmApiBase::with_credentials(domain, key, secret, passphrase)?;

        let futures = Box::new(LnmFuturesRepository::new(base.clone()));
        let user = Box::new(LnmUserRepository::new(base));

        Ok(Self { futures, user })
    }
}
