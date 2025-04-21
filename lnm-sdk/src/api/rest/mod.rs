pub mod error;
mod lnm;
pub mod models;
mod repositories;

use error::Result;
use lnm::{base::LnmApiBase, futures::LnmFuturesRepository};
use repositories::FuturesRepository;

pub struct RestApiContext {
    pub futures: Box<dyn FuturesRepository>,
}

impl RestApiContext {
    pub fn new(domain: String, key: String, secret: String, passphrase: String) -> Result<Self> {
        let base = LnmApiBase::new(domain, key, secret, passphrase)?;
        let futures = Box::new(LnmFuturesRepository::new(base));
        Ok(Self { futures })
    }
}
