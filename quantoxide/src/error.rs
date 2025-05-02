use std::result;
use thiserror::Error;

use crate::{db::error::DbError, sync::error::SyncError};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("[Sync] {0}")]
    Sync(#[from] SyncError),
    #[error("[Db] {0}")]
    Db(#[from] DbError),
}

pub type Result<T> = result::Result<T, AppError>;
