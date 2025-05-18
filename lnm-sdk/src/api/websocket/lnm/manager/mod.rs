use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{Mutex, broadcast, mpsc, oneshot},
    time,
};

use super::super::{
    error::{Result, WebSocketApiError},
    models::{ConnectionState, LnmJsonRpcRequest, LnmJsonRpcResponse, WebSocketApiRes},
};

mod connection;

use connection::{LnmWebSocketResponse, WebSocketApiConnection};

type PendingMap = HashMap<String, (LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type ShutdownTransmiter = mpsc::Sender<()>; // select! doesn't handle oneshot well
type ShutdownReceiver = mpsc::Receiver<()>;

pub type RequestTransmiter = mpsc::Sender<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;
type RequestReceiver = mpsc::Receiver<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type ResponseTransmiter = broadcast::Sender<WebSocketApiRes>;
pub type ResponseReceiver = broadcast::Receiver<WebSocketApiRes>;

pub struct ManagerTask {
    ws: WebSocketApiConnection,
    shutdown_rx: ShutdownReceiver,
    request_rx: RequestReceiver,
    responses_tx: ResponseTransmiter,
    connection_state: Arc<Mutex<Arc<ConnectionState>>>,
}

impl ManagerTask {
    pub async fn new(
        api_domain: String,
    ) -> Result<(
        Self,
        ShutdownTransmiter,
        RequestTransmiter,
        ResponseTransmiter,
        Arc<Mutex<Arc<ConnectionState>>>,
    )> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        // Internal channel for shutdown signal
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Internal channel for JSON RPC requests
        let (request_tx, request_rx) =
            mpsc::channel::<(LnmJsonRpcRequest, oneshot::Sender<bool>)>(100);

        // External channel for API responses
        let (responses_tx, _) = broadcast::channel::<WebSocketApiRes>(100);

        let connection_state = Arc::new(Mutex::new(Arc::new(ConnectionState::Connected)));

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
                                LnmWebSocketResponse::JsonRpc(json_rpc_res) => {
                                    match json_rpc_res {
                                        LnmJsonRpcResponse::Confirmation { id, channels } => {
                                            if let Some((req, oneshot_tx)) = pending.remove(&id) {
                                                let is_success = req.check_confirmation(&id, &channels);

                                                // Ignore errors resulting from dropped receivers
                                                let _ = oneshot_tx.send(is_success);
                                            }

                                            // Ignore unknown ids
                                        }
                                        LnmJsonRpcResponse::Subscription(data) => {
                                            // Ignore errors resulting from no receivers
                                            let _ = responses_tx.send(data.into());
                                        }
                                    }
                                }
                                LnmWebSocketResponse::Ping(payload) => {
                                    // Automatically respond to pings with pongs
                                    ws.send_pong(payload).await?;
                                }
                                // Closes are handled at `manager_task`
                                LnmWebSocketResponse::Close => {
                                    if shutdown_initiated {
                                        // Shutdown confirmation response received
                                        return Ok(());
                                    }

                                    // Server requested shutdown. Attempt to send close confirmation response
                                    let _ = ws.send_close().await;

                                    return Err(WebSocketApiError::ServerRequestedShutdown);
                                }
                                // Pongs can be ignored since heartbeat mechanism is reset after any message
                                LnmWebSocketResponse::Pong => {}
                            };
                        }
                        _ = &mut heartbeat_timer => {
                            if shutdown_initiated {
                                // No shutdown confirmation after a heartbeat, timeout
                                return Err(WebSocketApiError::NoServerShutdownConfirmation);
                            }

                            if waiting_for_pong {
                                // No pong received after ping and a heartbeat, timeout
                                return Err(WebSocketApiError::NoServerPong);
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

        let new_connection_state = match handler().await {
            Ok(_) => Arc::new(ConnectionState::Disconnected),
            Err(err) => Arc::new(ConnectionState::Failed(err)),
        };

        let mut connection_state_guard = self.connection_state.lock().await;
        *connection_state_guard = new_connection_state.clone();
        drop(connection_state_guard);

        // Notify all pending RPC requests of failure on shutdown
        for (_, (_, oneshot_tx)) in pending {
            // Ignore errors resulting from dropped receivers
            let _ = oneshot_tx.send(false);
        }

        let connection_update = WebSocketApiRes::from(new_connection_state);

        // Ignore errors resulting from no receivers
        let _ = self.responses_tx.send(connection_update);

        Ok(())
    }
}
