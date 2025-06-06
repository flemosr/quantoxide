use std::result;

use thiserror::Error;
use tokio::task::JoinError;

use lnm_sdk::api::rest::error::RestApiError;

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("ManagerNotReady error")]
    ManagerNotReady,

    #[error("ManagerNotViable error")]
    ManagerNotViable,

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
