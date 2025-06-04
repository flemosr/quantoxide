use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use super::super::{
    error::{Result, WebSocketApiError},
    models::{LnmJsonRpcRequest, LnmJsonRpcResponse, WebSocketApiRes},
    state::{ConnectionState, ConnectionStateManager},
};

mod connection;

use connection::{LnmWebSocketResponse, WebSocketApiConnection};

const WS_HEARTBEAT_SECS: u64 = 5;

type PendingMap = HashMap<String, (LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type DisconnectTransmiter = mpsc::Sender<()>;
type DisconnectReceiver = mpsc::Receiver<()>;

pub type RequestTransmiter = mpsc::Sender<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;
type RequestReceiver = mpsc::Receiver<(LnmJsonRpcRequest, oneshot::Sender<bool>)>;

pub type ResponseTransmiter = broadcast::Sender<WebSocketApiRes>;
pub type ResponseReceiver = broadcast::Receiver<WebSocketApiRes>;

pub struct WebSocketEventLoop {
    ws: WebSocketApiConnection,
    disconnect_rx: DisconnectReceiver,
    request_rx: RequestReceiver,
    responses_tx: ResponseTransmiter,
    connection_state_manager: Arc<ConnectionStateManager>,
}

impl WebSocketEventLoop {
    pub async fn new(
        api_domain: String,
        disconnect_rx: DisconnectReceiver,
        request_rx: RequestReceiver,
        response_tx: ResponseTransmiter,
        connection_state_manager: Arc<ConnectionStateManager>,
    ) -> Result<Self> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        Ok(Self {
            ws,
            disconnect_rx,
            request_rx,
            responses_tx: response_tx.clone(),
            connection_state_manager,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        let mut ws = self.ws;

        let mut pending: PendingMap = HashMap::new();

        let handler = || {
            let pending = &mut pending;
            let responses_tx = &self.responses_tx;

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
                                // Closes are handled at `manager_task`
                                LnmWebSocketResponse::Close => {
                                    if close_initiated {
                                        // Close confirmation response received
                                        return Ok(());
                                    }

                                    // Server requested close. Attempt to send close confirmation response
                                    let _ = ws.send_close().await;

                                    return Err(WebSocketApiError::ServerRequestedClose);
                                }
                                // Pongs can be ignored since heartbeat mechanism is reset after any message
                                LnmWebSocketResponse::Pong => {}
                            };
                        }
                        _ = &mut heartbeat_timer => {
                            if close_initiated {
                                // No close confirmation after a heartbeat, timeout
                                return Err(WebSocketApiError::NoServerCloseConfirmation);
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
            Ok(_) => ConnectionState::Disconnected,
            Err(err) => ConnectionState::Failed(err),
        };

        self.connection_state_manager.update(new_connection_state);

        // Notify all pending RPC requests of failure on shutdown
        for (_, (_, oneshot_tx)) in pending {
            // Ignore errors resulting from dropped receivers
            let _ = oneshot_tx.send(false);
        }

        let connection_update = WebSocketApiRes::from(self.connection_state_manager.snapshot());

        // Ignore errors resulting from no receivers
        let _ = self.responses_tx.send(connection_update);

        Ok(())
    }
}
