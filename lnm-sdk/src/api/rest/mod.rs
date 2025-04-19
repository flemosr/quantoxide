pub mod error;
mod lnm;
pub mod models;
mod repositories;

use error::Result;
use lnm::futures::LnmFuturesRepository;
use repositories::FuturesRepository;

pub struct RestApiContext {
    pub futures: Box<dyn FuturesRepository>,
}

impl RestApiContext {
    pub fn new(api_domain: String, api_secret: String) -> Result<Self> {
        let futures = Box::new(LnmFuturesRepository::new(api_domain, api_secret)?);
        Ok(Self { futures })
    }
}
