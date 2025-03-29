use fastwebsockets::{handshake, FragmentCollector, Frame, OpCode, WebSocketError};
use http_body_util::Empty;
use hyper::{
    body::Bytes,
    header::{CONNECTION, UPGRADE},
    upgrade::Upgraded,
    Request,
};
use hyper_util::rt::TokioIo;
use std::{future::Future, sync::Arc};
use tokio::net::TcpStream;
use tokio_rustls::{
    rustls::{pki_types::ServerName, ClientConfig, RootCertStore},
    TlsConnector,
};
use webpki_roots::TLS_SERVER_ROOTS;

use super::{
    error::{Result, WebSocketApiError},
    models::{JsonRpcResponse, LnmJsonRpcRequest},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketResponse {
    Close,
    JsonRpc(JsonRpcResponse),
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

fn tls_connector() -> Result<TlsConnector> {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.extend(TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

pub struct WebSocketApiConnection(FragmentCollector<TokioIo<Upgraded>>);

impl WebSocketApiConnection {
    pub async fn new(api_domain: String) -> Result<Self> {
        let api_addr = format!("{api_domain}:443");
        let api_uri = format!("wss://{api_domain}/");

        let api_domain = ServerName::try_from(api_domain.to_string())
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let tls_connector = tls_connector()?;
        let tcp_stream = TcpStream::connect(&api_addr)
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let tls_stream = tls_connector
            .connect(api_domain, tcp_stream)
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        let req = Request::builder()
            .method("GET")
            .uri(api_uri)
            .header("Host", &api_addr)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header("Sec-WebSocket-Key", handshake::generate_key())
            .header("Sec-WebSocket-Version", "13")
            .body(Empty::<Bytes>::new())
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        let (ws, _) = handshake::client(&SpawnExecutor, req, tls_stream)
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let ws = FragmentCollector::new(ws);

        Ok(Self(ws))
    }

    async fn send_frame(&mut self, frame: Frame<'_>) -> Result<()> {
        self.0
            .write_frame(frame)
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))
    }

    pub async fn send_json_rpc(&mut self, req: LnmJsonRpcRequest) -> Result<()> {
        let request_bytes = req
            .try_to_bytes()
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        let frame = Frame::text(request_bytes.into());
        self.send_frame(frame).await
    }

    pub async fn send_close(&mut self) -> Result<()> {
        let frame = Frame::close(1000, &[]);
        self.send_frame(frame).await
    }

    pub async fn send_pong(&mut self, payload: Vec<u8>) -> Result<()> {
        let frame = Frame::pong(payload.into());
        self.send_frame(frame).await
    }

    pub async fn send_ping(&mut self) -> Result<()> {
        let frame = Frame::new(true, OpCode::Ping, None, Vec::new().into());
        self.send_frame(frame).await
    }

    pub async fn read_respose(&mut self) -> Result<WebSocketResponse> {
        let frame = match self.0.read_frame().await {
            Ok(frame) => frame,
            // Expect scenario where connection is closed before shutdown confirmation response
            Err(WebSocketError::ConnectionClosed) => return Ok(WebSocketResponse::Close),
            Err(err) => return Err(WebSocketApiError::Generic(format!("frame error {:?}", err))),
        };

        let res = match frame.opcode {
            OpCode::Text => {
                let text = String::from_utf8(frame.payload.to_vec())
                    .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                let json_rpc_res = serde_json::from_str::<JsonRpcResponse>(&text)
                    .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

                WebSocketResponse::JsonRpc(json_rpc_res)
            }
            OpCode::Close => WebSocketResponse::Close,
            OpCode::Ping => WebSocketResponse::Ping(frame.payload.to_vec()),
            OpCode::Pong => WebSocketResponse::Pong,
            unhandled_opcode => {
                return Err(WebSocketApiError::Generic(format!(
                    "unhandled opcode {:?}",
                    unhandled_opcode
                )));
            }
        };

        Ok(res)
    }
}
