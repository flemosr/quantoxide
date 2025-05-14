use std::result;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for LiveError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for LiveError {}

pub type Result<T> = result::Result<T, LiveError>;
