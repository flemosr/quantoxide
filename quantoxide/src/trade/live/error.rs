use std::result;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LiveTradeError {
    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for LiveTradeError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for LiveTradeError {}

pub type Result<T> = result::Result<T, LiveTradeError>;
