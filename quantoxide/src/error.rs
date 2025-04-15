use std::result;
use thiserror::Error;

use crate::{db::error::DbError, sync::error::SyncError};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Sync error: {0}")]
    Sync(#[from] SyncError),
    #[error("Database error: {0}")]
    Db(#[from] DbError),
}

pub type Result<T> = result::Result<T, AppError>;
