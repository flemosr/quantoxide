use connection::{WebSocketApiConnection, WebSocketResponse};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex},
    task::JoinHandle,
    time::{self},
};

mod connection;
pub mod error;
pub mod models;

use error::{Result, WebSocketApiError};
use models::{LnmJsonRpcReqMethod, LnmJsonRpcRequest, LnmWebSocketChannel, WebSocketApiRes};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChannelStatus {
    SubscriptionPending,
    Subscribed,
    UnsubscriptionPending,
}

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Connected,
    Failed(WebSocketApiError),
    Disconnected,
}

pub struct WebSocketAPI {
    manager_handle: JoinHandle<Result<()>>,
    shutdown_sender: mpsc::Sender<()>, // select! doesn't handle oneshot well
    sub_sender: mpsc::Sender<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>,
    unsub_sender: mpsc::Sender<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>,
    res_sender: broadcast::Sender<WebSocketApiRes>,
    connection_state: Arc<Mutex<ConnectionState>>,
    subscriptions: Arc<Mutex<HashMap<LnmWebSocketChannel, ChannelStatus>>>,
}

impl WebSocketAPI {
    async fn handle_subscription_request(
        ws: &mut WebSocketApiConnection,
        pending_subs: &mut HashMap<String, oneshot::Sender<bool>>,
        req: (Vec<LnmWebSocketChannel>, oneshot::Sender<bool>),
    ) -> Result<()> {
        let (channels, oneshot_tx) = req;

        let req = LnmJsonRpcRequest::new(LnmJsonRpcReqMethod::Subscribe, channels);
        let req_id = req.id().to_string();

        ws.send_json_rpc(req).await?;

        pending_subs.insert(req_id, oneshot_tx);

        Ok(())
    }

    async fn handle_unsubscription_request(
        ws: &mut WebSocketApiConnection,
        pending_unsubs: &mut HashMap<String, oneshot::Sender<bool>>,
        req: (Vec<LnmWebSocketChannel>, oneshot::Sender<bool>),
    ) -> Result<()> {
        let (channels, oneshot_tx) = req;

        let req = LnmJsonRpcRequest::new(LnmJsonRpcReqMethod::Unsubscribe, channels);
        let req_id = req.id().to_string();

        ws.send_json_rpc(req).await?;

        pending_unsubs.insert(req_id, oneshot_tx);

        Ok(())
    }

    async fn handle_ws_response(
        ws: &mut WebSocketApiConnection,
        pending_subs: &mut HashMap<String, oneshot::Sender<bool>>,
        pending_unsubs: &mut HashMap<String, oneshot::Sender<bool>>,
        res_sender: &broadcast::Sender<WebSocketApiRes>,
        response: WebSocketResponse,
    ) -> Result<()> {
        match response {
            WebSocketResponse::JsonRpc(json_rpc_res) => {
                if let Some(id) = json_rpc_res.id.as_ref() {
                    if let Some(oneshot_tx) = pending_subs.remove(id) {
                        // This is a subscription confirmation response

                        // TODO: Check if subscription was successfull
                        let is_success = true;

                        oneshot_tx
                            .send(is_success)
                            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                    } else if let Some(oneshot_tx) = pending_unsubs.remove(id) {
                        // This is a unsubscription confirmation response

                        // TODO: Check if unsubscription was successfull
                        let is_success = true;

                        oneshot_tx
                            .send(is_success)
                            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                    }

                    // Ignore unhandled ids
                } else if let Some(method) = &json_rpc_res.method {
                    // TODO: Use proper method enum
                    if method == "subscription" {
                        let data = json_rpc_res.try_into()?;

                        res_sender
                            .send(data)
                            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
                    }
                } else {
                    return Err(WebSocketApiError::Generic(format!(
                        "unhandled json_rpc_res {:?}",
                        json_rpc_res
                    )));
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

    async fn manager_task(
        mut ws: WebSocketApiConnection,
        mut shutdown_receiver: mpsc::Receiver<()>,
        mut sub_receiver: mpsc::Receiver<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>,
        mut unsub_receiver: mpsc::Receiver<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>,
        msg_sender: broadcast::Sender<WebSocketApiRes>,
        connection_state: Arc<Mutex<ConnectionState>>,
    ) -> Result<()> {
        let mut pending_subs: HashMap<String, oneshot::Sender<bool>> = HashMap::new();
        let mut pending_unsubs: HashMap<String, oneshot::Sender<bool>> = HashMap::new();

        let handler = || {
            let pending_subs = &mut pending_subs;
            let pending_unsubs = &mut pending_unsubs;
            let msg_sender = &msg_sender;

            async move {
                let new_heartbeat_timer = || Box::pin(time::sleep(time::Duration::from_secs(5)));
                let mut heartbeat_timer = new_heartbeat_timer();
                let mut waiting_for_pong = false;
                let mut shutdown_initiated = false;

                loop {
                    tokio::select! {
                        Some(_) = shutdown_receiver.recv() => {
                            shutdown_initiated = true;
                            heartbeat_timer = new_heartbeat_timer();

                            ws.send_close().await?;
                        }
                        Some(req) = sub_receiver.recv() => {
                            Self::handle_subscription_request(&mut ws, pending_subs, req).await?;
                        }
                        Some(req) = unsub_receiver.recv() => {
                            Self::handle_unsubscription_request(&mut ws, pending_unsubs, req).await?;
                        }
                        read_res = ws.read_respose() => {
                            let response = read_res?;
                            let is_close_response = response == WebSocketResponse::Close;

                            // Reset heartbeat mechanism after receiving any message
                            waiting_for_pong = false;
                            heartbeat_timer = new_heartbeat_timer();

                            Self::handle_ws_response(
                                &mut ws,
                                pending_subs,
                                pending_unsubs,
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

        // Notify all pending subscriptions of failure on shutdown
        for (_, oneshot_tx) in pending_subs {
            let _ = oneshot_tx.send(false);
        }

        let mut connection_state_guard = connection_state.lock().await;
        *connection_state_guard = match handler_res {
            Err(err) => ConnectionState::Failed(err),
            Ok(_) => ConnectionState::Disconnected,
        };

        let connection_update = WebSocketApiRes::from(&*connection_state_guard);

        msg_sender.send(connection_update).map_err(|e| {
            WebSocketApiError::Generic(format!("Failed to send connection update {:?}", e))
        })?;

        Ok(())
    }

    pub async fn new(api_domain: String) -> Result<Self> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        // Internal channel for shutdown signal
        let (shutdown_sender, shutdown_receiver) = mpsc::channel::<()>(1);

        // Internal channel for subscription requests
        let (sub_sender, sub_receiver) =
            mpsc::channel::<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>(100);

        // Internal channel for unsubscription requests
        let (unsub_sender, unsub_receiver) =
            mpsc::channel::<(Vec<LnmWebSocketChannel>, oneshot::Sender<bool>)>(100);

        // External channel for API responses
        let (res_sender, _) = broadcast::channel::<WebSocketApiRes>(100);

        let connection_state = Arc::new(Mutex::new(ConnectionState::Connected));

        let manager_handle = tokio::spawn(Self::manager_task(
            ws,
            shutdown_receiver,
            sub_receiver,
            unsub_receiver,
            res_sender.clone(),
            connection_state.clone(),
        ));

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));

        Ok(WebSocketAPI {
            manager_handle,
            connection_state,
            shutdown_sender,
            sub_sender,
            unsub_sender,
            res_sender,
            subscriptions,
        })
    }

    pub fn is_connected(&self) -> bool {
        !self.manager_handle.is_finished()
    }

    pub async fn connection_state(&self) -> ConnectionState {
        self.connection_state.lock().await.clone()
    }

    async fn evaluate_manager_status(&self) -> Result<()> {
        let err = match self.connection_state().await {
            ConnectionState::Connected => return Ok(()),
            ConnectionState::Failed(err) => err,
            ConnectionState::Disconnected => {
                WebSocketApiError::Generic("WebSocket manager is finished".to_string())
            }
        };

        Err(err)
    }

    pub async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()> {
        self.evaluate_manager_status().await?;

        // Check current subscriptions
        let mut subscriptions_lock = self.subscriptions.lock().await;
        let mut channels_to_subscribe = Vec::new();

        for channel in channels {
            match subscriptions_lock.get(&channel) {
                Some(ChannelStatus::Subscribed | ChannelStatus::SubscriptionPending) => {
                    continue;
                }
                Some(ChannelStatus::UnsubscriptionPending) => {
                    return Err(WebSocketApiError::Generic(format!(
                        "Channel {channel} is pending unsubscription"
                    )));
                }
                None => {
                    // New subscription
                    channels_to_subscribe.push(channel.clone());
                    subscriptions_lock.insert(channel, ChannelStatus::SubscriptionPending);
                }
            }
        }

        drop(subscriptions_lock);

        // If no channels to subscribe, return success
        if channels_to_subscribe.is_empty() {
            return Ok(());
        }

        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        // Send subscription request to the manager task
        self.sub_sender
            .send((channels_to_subscribe.clone(), oneshot_tx))
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        let mut subscriptions_lock = self.subscriptions.lock().await;

        for channel in channels_to_subscribe {
            let channel_status = subscriptions_lock.get(&channel).ok_or_else(|| {
                WebSocketApiError::Generic("Invalid subscriptions state".to_string())
            })?;

            if *channel_status != ChannelStatus::SubscriptionPending {
                return Err(WebSocketApiError::Generic(
                    "Invalid subscriptions state".to_string(),
                ));
            }

            if success {
                subscriptions_lock.insert(channel, ChannelStatus::Subscribed);
            } else {
                subscriptions_lock.remove(&channel);
            }
        }

        Ok(())
    }

    pub async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()> {
        self.evaluate_manager_status().await?;

        let mut subscriptions_lock = self.subscriptions.lock().await;
        let mut channels_to_unsubscribe = Vec::new();

        for channel in channels {
            match subscriptions_lock.get(&channel) {
                Some(ChannelStatus::Subscribed) => {
                    // New subscription
                    channels_to_unsubscribe.push(channel.clone());
                    subscriptions_lock.insert(channel, ChannelStatus::UnsubscriptionPending);
                }
                Some(ChannelStatus::SubscriptionPending) => {
                    return Err(WebSocketApiError::Generic(format!(
                        "Channel {channel} is pending subscription"
                    )));
                }
                Some(ChannelStatus::UnsubscriptionPending) | None => {
                    continue;
                }
            }
        }

        drop(subscriptions_lock);

        // If no channels to subscribe, return success
        if channels_to_unsubscribe.is_empty() {
            return Ok(());
        }

        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        // Send subscription request to the manager task
        self.unsub_sender
            .send((channels_to_unsubscribe.clone(), oneshot_tx))
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;

        let mut subscriptions_lock = self.subscriptions.lock().await;

        for channel in channels_to_unsubscribe {
            let channel_status = subscriptions_lock.get(&channel).ok_or_else(|| {
                WebSocketApiError::Generic("Invalid subscriptions state".to_string())
            })?;

            if *channel_status != ChannelStatus::UnsubscriptionPending {
                return Err(WebSocketApiError::Generic(
                    "Invalid subscriptions state".to_string(),
                ));
            }

            if success {
                subscriptions_lock.remove(&channel);
            } else {
                subscriptions_lock.insert(channel, ChannelStatus::Subscribed);
            }
        }

        Ok(())
    }

    pub async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel> {
        let subscriptions = self.subscriptions.lock().await;
        subscriptions
            .iter()
            .filter_map(|(channel, status)| {
                if let ChannelStatus::Subscribed = status {
                    Some(channel.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<LnmWebSocketChannel>>()
    }

    pub async fn receiver(&self) -> Result<broadcast::Receiver<WebSocketApiRes>> {
        self.evaluate_manager_status().await?;

        let broadcast_rx = self.res_sender.subscribe();
        Ok(broadcast_rx)
    }

    pub async fn shutdown(self) -> Result<()> {
        if !self.manager_handle.is_finished() {
            self.shutdown_sender
                .send(())
                .await
                .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        }

        self.manager_handle
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?
    }
}
