use fastwebsockets::{OpCode, WebSocketError};
use hyper::http;
use std::{io, result, string::FromUtf8Error, sync::Arc};
use thiserror::Error;
use tokio_rustls::rustls::pki_types::InvalidDnsNameError;

use super::models::ConnectionState;

#[derive(Error, Debug)]
pub enum WebSocketApiError {
    #[error("BadConnectionState error")]
    BadConnectionState(Arc<ConnectionState>),
    #[error("InvalidDnsName error")]
    InvalidDnsName(InvalidDnsNameError),
    #[error("CreateTcpStream error")]
    CreateTcpStream(io::Error),
    #[error("ConnectTcpStream error")]
    ConnectTcpStream(io::Error),
    #[error("HttpUpgradeRequest error")]
    HttpUpgradeRequest(http::Error),
    #[error("Handshake error")]
    Handshake(WebSocketError),
    #[error("WriteFrame error")]
    WriteFrame(WebSocketError),
    #[error("EncodeJson error")]
    EncodeJson(serde_json::Error),
    #[error("ReadFrame error")]
    ReadFrame(WebSocketError),
    #[error("DecodeText error")]
    DecodeText(FromUtf8Error),
    #[error("DecodeJson error")]
    DecodeJson(serde_json::Error),
    #[error("UnhandledOpCode error")]
    UnhandledOpCode(OpCode),
    #[error("WebSocket generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, WebSocketApiError>;
