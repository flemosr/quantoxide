use std::{future::Future, sync::Arc};

use fastwebsockets::{FragmentCollector, Frame, OpCode, WebSocketError, handshake};
use http_body_util::Empty;
use hyper::{
    Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
    upgrade::Upgraded,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use webpki_roots::TLS_SERVER_ROOTS;

use super::super::super::{
    error::{ConnectionResult, WebSocketConnectionError},
    models::{LnmJsonRpcRequest, LnmJsonRpcResponse},
};

#[derive(Clone, Debug)]
pub enum LnmWebSocketResponse {
    Close,
    JsonRpc(LnmJsonRpcResponse),
    Ping(Vec<u8>),
    Pong,
}

struct SpawnExecutor;

impl<Fut> hyper::rt::Executor<Fut> for SpawnExecutor
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    fn execute(&self, fut: Fut) {
        tokio::task::spawn(fut);
    }
}

pub struct WebSocketApiConnection(FragmentCollector<TokioIo<Upgraded>>);

impl WebSocketApiConnection {
    pub async fn new(api_domain: String) -> ConnectionResult<Self> {
        let api_addr = format!("{api_domain}:443");
        let api_uri = format!("wss://{api_domain}/");

        let api_domain = ServerName::try_from(api_domain.to_string())
            .map_err(WebSocketConnectionError::InvalidDnsName)?;

        let tls_connector = {
            let mut root_cert_store = RootCertStore::empty();
            root_cert_store.extend(TLS_SERVER_ROOTS.iter().cloned());

            let config = ClientConfig::builder()
                .with_root_certificates(root_cert_store)
                .with_no_client_auth();

            TlsConnector::from(Arc::new(config))
        };

        let tcp_stream = TcpStream::connect(&api_addr)
            .await
            .map_err(WebSocketConnectionError::CreateTcpStream)?;
        let tls_stream = tls_connector
            .connect(api_domain, tcp_stream)
            .await
            .map_err(WebSocketConnectionError::ConnectTcpStream)?;

        let req = Request::builder()
            .method("GET")
            .uri(api_uri)
            .header("Host", &api_addr)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header("Sec-WebSocket-Key", handshake::generate_key())
            .header("Sec-WebSocket-Version", "13")
            .body(Empty::<Bytes>::new())
            .map_err(WebSocketConnectionError::HttpUpgradeRequest)?;

        let (ws, _) = handshake::client(&SpawnExecutor, req, tls_stream)
            .await
            .map_err(WebSocketConnectionError::Handshake)?;
        let ws = FragmentCollector::new(ws);

        Ok(Self(ws))
    }

    async fn send_frame(&mut self, frame: Frame<'_>) -> ConnectionResult<()> {
        self.0
            .write_frame(frame)
            .await
            .map_err(WebSocketConnectionError::WriteFrame)
    }

    pub async fn send_json_rpc(&mut self, req: LnmJsonRpcRequest) -> ConnectionResult<()> {
        let payload = req.try_into_bytes()?.into();
        let frame = Frame::text(payload);
        self.send_frame(frame).await
    }

    pub async fn send_close(&mut self) -> ConnectionResult<()> {
        let frame = Frame::close(1000, &[]);
        self.send_frame(frame).await
    }

    pub async fn send_pong(&mut self, payload: Vec<u8>) -> ConnectionResult<()> {
        let frame = Frame::pong(payload.into());
        self.send_frame(frame).await
    }

    pub async fn send_ping(&mut self) -> ConnectionResult<()> {
        let frame = Frame::new(true, OpCode::Ping, None, Vec::new().into());
        self.send_frame(frame).await
    }

    pub async fn read_respose(&mut self) -> ConnectionResult<LnmWebSocketResponse> {
        let frame = match self.0.read_frame().await {
            Ok(frame) => frame,
            // Expect scenario where connection is closed before close confirmation response
            Err(WebSocketError::ConnectionClosed) => return Ok(LnmWebSocketResponse::Close),
            Err(e) => return Err(WebSocketConnectionError::ReadFrame(e)),
        };

        let res = match frame.opcode {
            OpCode::Text => {
                let text = String::from_utf8(frame.payload.to_vec())
                    .map_err(WebSocketConnectionError::DecodeText)?;
                let json_rpc_res = serde_json::from_str::<LnmJsonRpcResponse>(&text)
                    .map_err(WebSocketConnectionError::DecodeJson)?;
                LnmWebSocketResponse::JsonRpc(json_rpc_res)
            }
            OpCode::Close => LnmWebSocketResponse::Close,
            OpCode::Ping => LnmWebSocketResponse::Ping(frame.payload.to_vec()),
            OpCode::Pong => LnmWebSocketResponse::Pong,
            unhandled_opcode => {
                return Err(WebSocketConnectionError::UnhandledOpCode(unhandled_opcode));
            }
        };

        Ok(res)
    }
}
