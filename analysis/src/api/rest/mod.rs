pub mod error;
mod lnm;
pub mod models;
mod repositories;

use lnm::futures::LnmFuturesRepository;
use repositories::FuturesRepository;

pub struct RestApiContext {
    pub futures: Box<dyn FuturesRepository>,
}

impl RestApiContext {
    pub fn new(api_domain: String) -> Self {
        let futures = Box::new(LnmFuturesRepository::new(api_domain));
        Self { futures }
    }
}
