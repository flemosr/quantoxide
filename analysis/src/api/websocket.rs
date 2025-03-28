use fastwebsockets::{handshake, FragmentCollector, Frame, OpCode, WebSocketError};
use futures::future::Either;
use http_body_util::Empty;
use hyper::{
    body::Bytes,
    header::{CONNECTION, UPGRADE},
    upgrade::Upgraded,
    Request,
};
use hyper_util::rt::TokioIo;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt, future::Future, pin::Pin, sync::Arc};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
    time::{self, Sleep},
};
use tokio_rustls::{
    rustls::{pki_types::ServerName, ClientConfig, RootCertStore},
    TlsConnector,
};
use webpki_roots::TLS_SERVER_ROOTS;

use super::error::{ApiError, Result};

enum LnmJsonRpcMethod {
    Subscribe,
    Unsubscribe,
}

impl LnmJsonRpcMethod {
    fn as_str(&self) -> &'static str {
        match self {
            LnmJsonRpcMethod::Subscribe => "v1/public/subscribe",
            LnmJsonRpcMethod::Unsubscribe => "v1/public/unsubscribe",
        }
    }
}

impl fmt::Display for LnmJsonRpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    id: String,
    params: Vec<String>,
}

impl JsonRpcRequest {
    pub fn new(method: LnmJsonRpcMethod, params: Vec<String>) -> Self {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill(&mut random_bytes);
        let request_id = hex::encode(random_bytes);

        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            id: request_id,
            params,
        }
    }

    fn try_to_bytes(&self) -> Result<Vec<u8>> {
        let request_json =
            serde_json::to_string(&self).map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let bytes = request_json.into_bytes();
        Ok(bytes)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<String>,
    method: Option<String>,
    result: Option<Value>,
    error: Option<Value>,
    params: Option<Value>,
}

pub enum LnmWebSocketChannels {
    FuturesBtcUsdIndex,
    FuturesBtcUsdLastPrice,
}

impl LnmWebSocketChannels {
    fn as_str(&self) -> &'static str {
        match self {
            LnmWebSocketChannels::FuturesBtcUsdIndex => "futures:btc_usd:index",
            LnmWebSocketChannels::FuturesBtcUsdLastPrice => "futures:btc_usd:last-price",
        }
    }
}

impl fmt::Display for LnmWebSocketChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
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

pub struct WebSocketAPI {
    manager_handle: JoinHandle<Result<()>>,
    shutdown_sender: mpsc::Sender<()>, // select! doesn't handle oneshot well
    sub_sender: mpsc::Sender<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>,
    unsub_sender: mpsc::Sender<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>,
    msg_sender: broadcast::Sender<JsonRpcResponse>,
}

impl WebSocketAPI {
    async fn connect() -> Result<FragmentCollector<TokioIo<Upgraded>>> {
        let api_domain = super::get_api_domain()?;
        let api_addr = format!("{api_domain}:443");
        let api_uri = format!("wss://{api_domain}/");

        let api_domain = ServerName::try_from(api_domain.to_string())
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let tls_connector = tls_connector()?;
        let tcp_stream = TcpStream::connect(&api_addr)
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let tls_stream = tls_connector
            .connect(api_domain, tcp_stream)
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        let req = Request::builder()
            .method("GET")
            .uri(api_uri)
            .header("Host", &api_addr)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header("Sec-WebSocket-Key", handshake::generate_key())
            .header("Sec-WebSocket-Version", "13")
            .body(Empty::<Bytes>::new())
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        let (ws, _) = handshake::client(&SpawnExecutor, req, tls_stream)
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let ws = FragmentCollector::new(ws);

        Ok(ws)
    }

    async fn handle_shutdown_signal(ws: &mut FragmentCollector<TokioIo<Upgraded>>) -> Result<()> {
        ws.write_frame(Frame::close(1000, &[]))
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))
    }

    async fn handle_subscription_request(
        ws: &mut FragmentCollector<TokioIo<Upgraded>>,
        pending_subs: &mut HashMap<String, oneshot::Sender<bool>>,
        req: (Vec<LnmWebSocketChannels>, oneshot::Sender<bool>),
    ) -> Result<()> {
        let (channels, oneshot_tx) = req;

        let channels = channels
            .into_iter()
            .map(|channel| channel.to_string())
            .collect();
        let req = JsonRpcRequest::new(LnmJsonRpcMethod::Subscribe, channels);

        let request_bytes = req
            .try_to_bytes()
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let frame = Frame::text(request_bytes.into());

        ws.write_frame(frame)
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        pending_subs.insert(req.id, oneshot_tx);

        Ok(())
    }

    async fn handle_unsubscription_request(
        ws: &mut FragmentCollector<TokioIo<Upgraded>>,
        pending_unsubs: &mut HashMap<String, oneshot::Sender<bool>>,
        req: (Vec<LnmWebSocketChannels>, oneshot::Sender<bool>),
    ) -> Result<()> {
        let (channels, oneshot_tx) = req;

        let channels = channels
            .into_iter()
            .map(|channel| channel.to_string())
            .collect();
        let req = JsonRpcRequest::new(LnmJsonRpcMethod::Unsubscribe, channels);

        let request_bytes = req
            .try_to_bytes()
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        let frame = Frame::text(request_bytes.into());

        ws.write_frame(frame)
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        pending_unsubs.insert(req.id, oneshot_tx);

        Ok(())
    }

    async fn handle_incoming_ws_frame(
        ws: &mut FragmentCollector<TokioIo<Upgraded>>,
        pending_subs: &mut HashMap<String, oneshot::Sender<bool>>,
        pending_unsubs: &mut HashMap<String, oneshot::Sender<bool>>,
        msg_sender: &broadcast::Sender<JsonRpcResponse>,
        shutdown_initiated: bool,
        frame_result: std::result::Result<Frame<'_>, WebSocketError>,
    ) -> Result<bool> {
        let frame = match frame_result {
            Ok(frame) => frame,
            // Expect scenario where connection is closed before shutdown confirmation response
            Err(WebSocketError::ConnectionClosed) if shutdown_initiated => return Ok(true),
            Err(err) => return Err(ApiError::WebSocketGeneric(format!("frame error {:?}", err))),
        };

        match frame.opcode {
            OpCode::Text => {
                let text = String::from_utf8(frame.payload.to_vec())
                    .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
                let json_rpc_res = serde_json::from_str::<JsonRpcResponse>(&text)
                    .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

                if let Some(id) = json_rpc_res.id.as_ref() {
                    if let Some(oneshot_tx) = pending_subs.remove(id) {
                        // This is a subscription confirmation response

                        // TODO: Check if subscription was successfull
                        let is_success = true;

                        oneshot_tx
                            .send(is_success)
                            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

                        return Ok(false);
                    } else if let Some(oneshot_tx) = pending_unsubs.remove(id) {
                        // This is a unsubscription confirmation response

                        // TODO: Check if unsubscription was successfull
                        let is_success = true;

                        oneshot_tx
                            .send(is_success)
                            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

                        return Ok(false);
                    }
                } else if let Some(method) = &json_rpc_res.method {
                    // Regular message; send to consumer
                    if method == "subscription" {
                        // TODO: check channel and parse it propertly

                        msg_sender
                            .send(json_rpc_res)
                            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

                        return Ok(false);
                    }
                }

                Err(ApiError::WebSocketGeneric(format!(
                    "unhandled text {:?}",
                    text
                )))
            }
            OpCode::Close => {
                if shutdown_initiated {
                    // Shutdown confirmation response received
                    return Ok(true);
                }

                // Send shutdown confirmation response
                let _ = Self::handle_shutdown_signal(ws).await;

                Err(ApiError::WebSocketGeneric(
                    "server requested shutdown".to_string(),
                ))
            }
            unhandled_opcode => Err(ApiError::WebSocketGeneric(format!(
                "unhandled opcode {:?}",
                unhandled_opcode
            ))),
        }
    }

    async fn manager_task(
        mut ws: FragmentCollector<TokioIo<Upgraded>>,
        mut shutdown_receiver: mpsc::Receiver<()>,
        mut sub_receiver: mpsc::Receiver<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>,
        mut unsub_receiver: mpsc::Receiver<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>,
        msg_sender: broadcast::Sender<JsonRpcResponse>,
    ) -> Result<()> {
        let mut pending_subs: HashMap<String, oneshot::Sender<bool>> = HashMap::new();
        let mut pending_unsubs: HashMap<String, oneshot::Sender<bool>> = HashMap::new();

        let interaction_loop = || {
            let pending_subs = &mut pending_subs;
            let pending_unsubs = &mut pending_unsubs;

            async move {
                let mut shutdown_initiated = false;
                let mut shutdown_timeout: Option<Pin<Box<Sleep>>> = None;

                loop {
                    tokio::select! {
                        Some(_) = shutdown_receiver.recv() => {
                            shutdown_initiated = true;

                            Self::handle_shutdown_signal(&mut ws).await?;

                            shutdown_timeout = Some(Box::pin(time::sleep(time::Duration::from_secs(5))));
                        }
                        _ = if let Some(timeout) = &mut shutdown_timeout {
                            Either::Left(timeout)
                        } else {
                            Either::Right(std::future::pending::<()>())
                        } => {
                            return Err(ApiError::WebSocketGeneric("shutdown timeout reached".to_string()));
                        }
                        Some(req) = sub_receiver.recv() => {
                            Self::handle_subscription_request(&mut ws, pending_subs, req).await?;
                        }
                        Some(req) = unsub_receiver.recv() => {
                            Self::handle_unsubscription_request(&mut ws, pending_unsubs, req).await?;
                        }
                        frame_result = ws.read_frame() => {
                            let is_close_signal = Self::handle_incoming_ws_frame(
                                &mut ws,
                                pending_subs,
                                pending_unsubs,
                                &msg_sender,
                                shutdown_initiated,
                                frame_result
                            ).await?;

                            if is_close_signal {
                                return Ok(());
                            }
                        }
                    };
                }
            }
        };

        let res = interaction_loop().await;

        // Notify all pending subscriptions of failure on shutdown
        for (_, oneshot_tx) in pending_subs {
            oneshot_tx
                .send(false)
                .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
        }

        res
    }

    pub async fn new() -> Result<Self> {
        let ws = Self::connect().await?;

        // Internal channel for shutdown signal
        let (shutdown_sender, shutdown_receiver) = mpsc::channel::<()>(1);

        // Internal channel for subscription requests
        let (sub_sender, sub_receiver) =
            mpsc::channel::<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>(100);

        // Internal channel for unsubscription requests
        let (unsub_sender, unsub_receiver) =
            mpsc::channel::<(Vec<LnmWebSocketChannels>, oneshot::Sender<bool>)>(100);

        // External channel for subscription messages
        let (msg_sender, _) = broadcast::channel::<JsonRpcResponse>(100);

        let manager_handle = tokio::spawn(Self::manager_task(
            ws,
            shutdown_receiver,
            sub_receiver,
            unsub_receiver,
            msg_sender.clone(),
        ));

        Ok(WebSocketAPI {
            manager_handle,
            shutdown_sender,
            sub_sender,
            unsub_sender,
            msg_sender,
        })
    }

    pub async fn shutdown(self) -> Result<()> {
        self.shutdown_sender
            .send(())
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        self.manager_handle
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?
    }

    pub async fn subscribe(&self, channels: Vec<LnmWebSocketChannels>) -> Result<()> {
        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        // Send subscription request to the manager task
        self.sub_sender
            .send((channels, oneshot_tx))
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        if !success {
            return Err(ApiError::WebSocketGeneric(
                "could not subscribe".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannels>) -> Result<()> {
        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        // Send unsubscription request to the manager task
        self.unsub_sender
            .send((channels, oneshot_tx))
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;

        if !success {
            return Err(ApiError::WebSocketGeneric(
                "could not unsubscribe".to_string(),
            ));
        }

        Ok(())
    }

    pub fn receiver(&self) -> broadcast::Receiver<JsonRpcResponse> {
        self.msg_sender.subscribe()
    }
}
