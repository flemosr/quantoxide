use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
    time,
};

use super::super::{
    error::{ConnectionResult, WebSocketConnectionError},
    models::{LnmJsonRpcRequest, LnmJsonRpcResponse, WebSocketUpdate},
    state::{ConnectionStatus, ConnectionStatusManager},
};

mod connection;

use connection::{LnmWebSocketResponse, WebSocketApiConnection};

const WS_HEARTBEAT_SECS: u64 = 5;

type PendingMap = HashMap<String, (LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub(super) type DisconnectTransmiter = mpsc::Sender<()>;
type DisconnectReceiver = mpsc::Receiver<()>;

pub(super) type RequestTransmiter = mpsc::Sender<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;
type RequestReceiver = mpsc::Receiver<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub(super) type ResponseTransmiter = broadcast::Sender<WebSocketUpdate>;
pub(super) type ResponseReceiver = broadcast::Receiver<WebSocketUpdate>;

pub(super) struct WebSocketEventLoop {
    ws: WebSocketApiConnection,
    disconnect_rx: DisconnectReceiver,
    request_rx: RequestReceiver,
    response_tx: ResponseTransmiter,
    connection_status_manager: Arc<ConnectionStatusManager>,
}

impl WebSocketEventLoop {
    async fn new(
        api_domain: String,
        disconnect_rx: DisconnectReceiver,
        request_rx: RequestReceiver,
        response_tx: ResponseTransmiter,
        connection_status_manager: Arc<ConnectionStatusManager>,
    ) -> ConnectionResult<Self> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        Ok(Self {
            ws,
            disconnect_rx,
            request_rx,
            response_tx,
            connection_status_manager,
        })
    }

    async fn run(mut self) {
        let mut ws = self.ws;

        let mut pending: PendingMap = HashMap::new();

        let handler = || {
            let pending = &mut pending;
            let responses_tx = &self.response_tx;

            async move {
                let new_heartbeat_timer =
                    || Box::pin(time::sleep(time::Duration::from_secs(WS_HEARTBEAT_SECS)));
                let mut heartbeat_timer = new_heartbeat_timer();
                let mut waiting_for_pong = false;
                let mut close_initiated = false;

                loop {
                    tokio::select! {
                        Some(_) = self.disconnect_rx.recv() => {
                            close_initiated = true;
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
                                LnmWebSocketResponse::Close => {
                                    if close_initiated {
                                        // Close confirmation response received
                                        return Ok(());
                                    }

                                    // Server requested close. Attempt to send close confirmation response
                                    let _ = ws.send_close().await;

                                    return Err(WebSocketConnectionError::ServerRequestedClose);
                                }
                                // Pongs can be ignored since heartbeat mechanism is reset after any message
                                LnmWebSocketResponse::Pong => {}
                            };
                        }
                        _ = &mut heartbeat_timer => {
                            if close_initiated {
                                // No close confirmation after a heartbeat, timeout
                                return Err(WebSocketConnectionError::NoServerCloseConfirmation);
                            }

                            if waiting_for_pong {
                                // No pong received after ping and a heartbeat, timeout
                                return Err(WebSocketConnectionError::NoServerPong);
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

        let new_connection_status = match handler().await {
            Ok(_) => ConnectionStatus::Disconnected,
            Err(e) => ConnectionStatus::Failed(e),
        };

        self.connection_status_manager.update(new_connection_status);

        // Notify all pending RPC requests of failure on shutdown
        for (_, (_, oneshot_tx)) in pending {
            // Ignore dropped receivers errors
            let _ = oneshot_tx.send(false);
        }

        let connection_update = self.connection_status_manager.snapshot();

        // Ignore no-receivers errors
        let _ = self.response_tx.send(connection_update.into());
    }

    pub async fn try_spawn(
        api_domain: String,
        disconnect_rx: DisconnectReceiver,
        request_rx: RequestReceiver,
        response_tx: ResponseTransmiter,
    ) -> ConnectionResult<(JoinHandle<()>, Arc<ConnectionStatusManager>)> {
        let connection_status_manager = ConnectionStatusManager::new();

        let event_loop = Self::new(
            api_domain,
            disconnect_rx,
            request_rx,
            response_tx,
            connection_status_manager.clone(),
        )
        .await?;

        let event_loop_handle = tokio::spawn(event_loop.run());

        Ok((event_loop_handle, connection_status_manager))
    }
}
