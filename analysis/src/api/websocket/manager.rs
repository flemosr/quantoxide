use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex},
    time,
};

use super::{
    connection::{WebSocketApiConnection, WebSocketResponse},
    error::{Result, WebSocketApiError},
    models::{LnmJsonRpcRequest, LnmJsonRpcResponse, WebSocketApiRes},
    ConnectionState,
};

type PendingMap = HashMap<String, (LnmJsonRpcRequest, oneshot::Sender<bool>)>;

async fn handle_ws_response(
    ws: &mut WebSocketApiConnection,
    pending: &mut PendingMap,
    responses_tx: &broadcast::Sender<WebSocketApiRes>,
    response: WebSocketResponse,
) -> Result<()> {
    match response {
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
        WebSocketResponse::Close => {}
        // Pongs can be ignored since heartbeat mechanism is reset after any message
        WebSocketResponse::Pong => {}
    };
    Ok(())
}

pub async fn task(
    mut ws: WebSocketApiConnection,
    mut shutdown_rx: mpsc::Receiver<()>,
    mut requests_rx: mpsc::Receiver<(LnmJsonRpcRequest, oneshot::Sender<bool>)>,
    responses_tx: broadcast::Sender<WebSocketApiRes>,
    connection_state: Arc<Mutex<ConnectionState>>,
) -> Result<()> {
    let mut pending: PendingMap = HashMap::new();

    let handler = || {
        let pending = &mut pending;
        let msg_sender = &responses_tx;

        async move {
            let new_heartbeat_timer = || Box::pin(time::sleep(time::Duration::from_secs(5)));
            let mut heartbeat_timer = new_heartbeat_timer();
            let mut waiting_for_pong = false;
            let mut shutdown_initiated = false;

            loop {
                tokio::select! {
                    Some(_) = shutdown_rx.recv() => {
                        shutdown_initiated = true;
                        heartbeat_timer = new_heartbeat_timer();

                        ws.send_close().await?;
                    }
                    Some((json_rpc_req, oneshot_tx)) = requests_rx.recv() => {
                        ws.send_json_rpc(json_rpc_req.clone()).await?;
                        pending.insert(json_rpc_req.id().clone(), (json_rpc_req, oneshot_tx));
                    }
                    read_res = ws.read_respose() => {
                        let response = read_res?;
                        let is_close_response = response == WebSocketResponse::Close;

                        // Reset heartbeat mechanism after receiving any message
                        waiting_for_pong = false;
                        heartbeat_timer = new_heartbeat_timer();

                        handle_ws_response(
                            &mut ws,
                            pending,
                            &msg_sender,
                            response
                        ).await?;

                        if !is_close_response {
                            continue;
                        }

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

    let mut connection_state_guard = connection_state.lock().await;
    *connection_state_guard = match handler_res {
        Err(err) => ConnectionState::Failed(err),
        Ok(_) => ConnectionState::Disconnected,
    };

    let connection_update = WebSocketApiRes::from(&*connection_state_guard);

    responses_tx.send(connection_update).map_err(|e| {
        WebSocketApiError::Generic(format!("Failed to send connection update {:?}", e))
    })?;

    Ok(())
}
