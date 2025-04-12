use fastwebsockets::{OpCode, WebSocketError};
use hyper::http;
use std::{io, result, string::FromUtf8Error, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast::error::SendError;
use tokio_rustls::rustls::pki_types::InvalidDnsNameError;

use super::models::{ConnectionState, WebSocketApiRes};

#[derive(Error, Debug)]
pub enum WebSocketApiError {
    #[error("BadConnectionState error, {0:?}")]
    BadConnectionState(Arc<ConnectionState>),
    #[error("InvalidDnsName error, {0}")]
    InvalidDnsName(InvalidDnsNameError),
    #[error("CreateTcpStream error, {0}")]
    CreateTcpStream(io::Error),
    #[error("ConnectTcpStream error, {0}")]
    ConnectTcpStream(io::Error),
    #[error("HttpUpgradeRequest error, {0}")]
    HttpUpgradeRequest(http::Error),
    #[error("Handshake error, {0}")]
    Handshake(WebSocketError),
    #[error("WriteFrame error, {0}")]
    WriteFrame(WebSocketError),
    #[error("EncodeJson error, {0}")]
    EncodeJson(serde_json::Error),
    #[error("ReadFrame error, {0}")]
    ReadFrame(WebSocketError),
    #[error("DecodeText error, {0}")]
    DecodeText(FromUtf8Error),
    #[error("DecodeJson error, {0}")]
    DecodeJson(serde_json::Error),
    #[error("UnhandledOpCode error, {0:?}")]
    UnhandledOpCode(OpCode),
    #[error("SubscriptionConfirmation error")]
    SubscriptionConfirmation,
    #[error("SubscriptionMessage error")]
    SubscriptionMessage(SendError<WebSocketApiRes>),
    #[error("ServerRequestedShutdown error")]
    ServerRequestedShutdown,
    #[error("NoShutdownConfirmation error")]
    NoShutdownConfirmation,
    #[error("NoPong error")]
    NoPong,
    #[error("ConnectionUpdate error")]
    ConnectionUpdate(SendError<WebSocketApiRes>),
    #[error("WebSocket generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, WebSocketApiError>;
