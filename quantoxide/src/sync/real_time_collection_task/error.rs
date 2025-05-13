use std::{result, sync::Arc};

use thiserror::Error;

use lnm_sdk::api::websocket::{error::WebSocketApiError, models::ConnectionState};

use crate::db::error::DbError;

#[derive(Error, Debug)]
pub enum RealTimeCollectionError {
    #[error("[WebSocketApi] {0}")]
    WebSocketApi(#[from] WebSocketApiError),

    #[error("BadConnectionUpdate error, {0:?}")]
    BadConnectionUpdate(Arc<ConnectionState>),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("RealTimeCollection generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, RealTimeCollectionError>;
