use std::{io, result, string::FromUtf8Error};

use fastwebsockets::{OpCode, WebSocketError};
use hyper::http;
use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinError,
};
use tokio_rustls::rustls::pki_types::InvalidDnsNameError;

use super::{
    lnm::ChannelStatus,
    models::{JsonRpcResponse, LnmJsonRpcRequest, LnmWebSocketChannel, WebSocketUpdate},
    state::WsConnectionStatus,
};

#[derive(Error, Debug)]
pub enum WebSocketConnectionError {
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

    #[error("ServerRequestedClose error")]
    ServerRequestedClose,

    #[error("NoServerCloseConfirmation error")]
    NoServerCloseConfirmation,

    #[error("NoServerPong error")]
    NoServerPong,

    #[error("UnexpectedJsonRpcResponse error, {0:?}")]
    UnexpectedJsonRpcResponse(JsonRpcResponse),
}

pub(crate) type ConnectionResult<T> = result::Result<T, WebSocketConnectionError>;

#[derive(Error, Debug)]
pub enum WebSocketApiError {
    #[error("Failed to spawn event loop: {0}")]
    FailedToSpawnEventLoop(WebSocketConnectionError),

    #[error("BadConnectionStatus error, {0}")]
    BadConnectionStatus(WsConnectionStatus),

    #[error("SendConnectionUpdate error, {0}")]
    SendConnectionUpdate(broadcast::error::SendError<WebSocketUpdate>),

    #[error("SubscribeWithUnsubscriptionPending error, {0}")]
    SubscribeWithUnsubscriptionPending(LnmWebSocketChannel),

    #[error("SendSubscriptionRequest error, {0}")]
    SendSubscriptionRequest(mpsc::error::SendError<(LnmJsonRpcRequest, oneshot::Sender<bool>)>),

    #[error("ReceiveSubscriptionConfirmation error, {0}")]
    ReceiveSubscriptionConfirmation(oneshot::error::RecvError),

    #[error("InvalidSubscriptionsChannelNotFound error, {0}")]
    InvalidSubscriptionsChannelNotFound(LnmWebSocketChannel),

    #[error("InvalidSubscriptionsChannelStatus error")]
    InvalidSubscriptionsChannelStatus {
        channel: LnmWebSocketChannel,
        status: ChannelStatus,
    },

    #[error("UnsubscribeWithSubscriptionPending error, {0}")]
    UnsubscribeWithSubscriptionPending(LnmWebSocketChannel),

    #[error("SendUnubscriptionRequest error, {0}")]
    SendUnubscriptionRequest(mpsc::error::SendError<(LnmJsonRpcRequest, oneshot::Sender<bool>)>),

    #[error("ReceiveUnsubscriptionConfirmation error")]
    ReceiveUnsubscriptionConfirmation(oneshot::error::RecvError),

    #[error("SendDisconnectRequest error, {0}")]
    SendDisconnectRequest(mpsc::error::SendError<()>),

    #[error("UnknownChannel error, {0}")]
    UnknownChannel(String),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("WebSocket is not connected, status: {0}")]
    WebSocketNotConnected(WsConnectionStatus),

    #[error("WebSocket disconnect timeout")]
    DisconnectTimeout,
}

pub(crate) type Result<T> = result::Result<T, WebSocketApiError>;
