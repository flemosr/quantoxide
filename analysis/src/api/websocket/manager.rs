use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex},
    time,
};

use super::{
    error::{Result, WebSocketApiError},
    models::{LnmJsonRpcRequest, LnmJsonRpcResponse, WebSocketApiRes},
};

mod connection;

use connection::{WebSocketApiConnection, WebSocketResponse};

type PendingMap = HashMap<String, (LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type ShutdownTransmiter = mpsc::Sender<()>; // select! doesn't handle oneshot well
type ShutdownReceiver = mpsc::Receiver<()>;

pub type RequestTransmiter = mpsc::Sender<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;
type RequestReceiver = mpsc::Receiver<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type ResponseTransmiter = broadcast::Sender<WebSocketApiRes>;
pub type ResponseReceiver = broadcast::Receiver<WebSocketApiRes>;

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Failed(WebSocketApiError),
}

pub struct ManagerTask {
    ws: WebSocketApiConnection,
    shutdown_rx: ShutdownReceiver,
    request_rx: RequestReceiver,
    responses_tx: ResponseTransmiter,
    connection_state: Arc<Mutex<ConnectionState>>,
}

impl ManagerTask {
    pub async fn new(
        api_domain: String,
    ) -> Result<(
        Self,
        ShutdownTransmiter,
        RequestTransmiter,
        ResponseTransmiter,
        Arc<Mutex<ConnectionState>>,
    )> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        // Internal channel for shutdown signal
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Internal channel for JSON RPC requests
        let (request_tx, request_rx) =
            mpsc::channel::<(LnmJsonRpcRequest, oneshot::Sender<bool>)>(100);

        // External channel for API responses
        let (responses_tx, _) = broadcast::channel::<WebSocketApiRes>(100);

        let connection_state = Arc::new(Mutex::new(ConnectionState::Connected));

        let manager = Self {
            ws,
            shutdown_rx,
            request_rx,
            responses_tx: responses_tx.clone(),
            connection_state: connection_state.clone(),
        };

        Ok((
            manager,
            shutdown_tx,
            request_tx,
            responses_tx,
            connection_state,
        ))
    }

    pub async fn run(mut self) -> Result<()> {
        let mut ws = self.ws;

        let mut pending: PendingMap = HashMap::new();

        let handler = || {
            let pending = &mut pending;
            let responses_tx = &self.responses_tx;

            async move {
                let new_heartbeat_timer = || Box::pin(time::sleep(time::Duration::from_secs(5)));
                let mut heartbeat_timer = new_heartbeat_timer();
                let mut waiting_for_pong = false;
                let mut shutdown_initiated = false;

                loop {
                    tokio::select! {
                        Some(_) = self.shutdown_rx.recv() => {
                            shutdown_initiated = true;
                            heartbeat_timer = new_heartbeat_timer();

                            ws.send_close().await?;
                        }
                        Some((json_rpc_req, oneshot_tx)) = self.request_rx.recv() => {
                            ws.send_json_rpc(json_rpc_req.clone()).await?;
                            pending.insert(json_rpc_req.id().clone(), (json_rpc_req, oneshot_tx));
                        }
                        read_response_result = ws.read_respose() => {
                            // Reset heartbeat mechanism after receiving any message
                            waiting_for_pong = false;
                            heartbeat_timer = new_heartbeat_timer();

                            match read_response_result? {
                                WebSocketResponse::JsonRpc(json_rpc_res) => {
                                    let lnm_json_rpc_res = LnmJsonRpcResponse::try_from(json_rpc_res)?;

                                    match lnm_json_rpc_res {
                                        LnmJsonRpcResponse::Confirmation { id, channels } => {
                                            if let Some((req, oneshot_tx)) = pending.remove(&id) {
                                                let is_success = req.id() == &id && req.channels() == &channels;

                                                oneshot_tx
                                                    .send(is_success)
                                                    .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                                            }

                                            // Ignore unknown ids
                                        }
                                        LnmJsonRpcResponse::Subscription(data) => {
                                            responses_tx
                                                .send(data)
                                                .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                                        }
                                    }
                                }
                                WebSocketResponse::Ping(payload) => {
                                    // Automatically respond to pings with pongs
                                    ws.send_pong(payload).await?;
                                }
                                // Closes are handled at `manager_task`
                                WebSocketResponse::Close => {
                                    if shutdown_initiated {
                                        // Shutdown confirmation response received
                                        return Ok(true);
                                    }

                                    // Server requested shutdown. Attempt to send close confirmation response
                                    // but don't handle potential errors since `WebSocketApiError::Generic`
                                    // will be returned bellow.
                                    let _ = ws.send_close().await;

                                    return Err(WebSocketApiError::Generic(
                                        "server requested shutdown".to_string(),
                                    ));
                                }
                                // Pongs can be ignored since heartbeat mechanism is reset after any message
                                WebSocketResponse::Pong => {}
                            };
                        }
                        _ = &mut heartbeat_timer => {
                            if shutdown_initiated {
                                // No shutdown confirmation after a heartbeat, timeout
                                return Err(WebSocketApiError::Generic("shutdown timeout reached".to_string()));
                            }

                            if waiting_for_pong {
                                // No pong received after ping and a heartbeat, timeout
                                return Err(WebSocketApiError::Generic("pong response timeout, connection may be dead".to_string()));
                            }

                            // No messages received for a heartbeat, send a ping
                            ws.send_ping().await?;

                            waiting_for_pong = true;
                            heartbeat_timer = new_heartbeat_timer();
                        }
                    };
                }
            }
        };

        let handler_res = handler().await;

        // Notify all pending RPC requests of failure on shutdown
        for (_, (_, oneshot_tx)) in pending {
            let _ = oneshot_tx.send(false);
        }

        let mut connection_state_guard = self.connection_state.lock().await;
        *connection_state_guard = match handler_res {
            Err(err) => ConnectionState::Failed(err),
            Ok(_) => ConnectionState::Disconnected,
        };

        let connection_update = WebSocketApiRes::from(&*connection_state_guard);

        self.responses_tx.send(connection_update).map_err(|e| {
            WebSocketApiError::Generic(format!("Failed to send connection update {:?}", e))
        })?;

        Ok(())
    }
}
