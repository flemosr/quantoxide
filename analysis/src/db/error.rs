use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Init error: {0}")]
    Init(&'static str),
    #[error("Connection error: {0}")]
    Connection(sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(sqlx::migrate::MigrateError),
    #[error("Query error: {0}")]
    Query(sqlx::Error),
    #[error("Transaction begin error: {0}")]
    TransactionBegin(sqlx::Error),
    #[error("Transaction commit error: {0}")]
    TransactionCommit(sqlx::Error),
}

pub type Result<T> = result::Result<T, DbError>;
