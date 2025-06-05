use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as SyncMutex},
};

use async_trait::async_trait;
use tokio::{
    sync::{Mutex as AsyncMutex, broadcast, mpsc, oneshot},
    task::JoinHandle,
    time,
};

use super::{
    WebSocketApiConfig,
    error::{Result, WebSocketApiError},
    models::{LnmJsonRpcReqMethod, LnmJsonRpcRequest, LnmWebSocketChannel, WebSocketApiRes},
    repositories::WebSocketRepository,
    state::{ConnectionState, ConnectionStateManager},
};

mod event_loop;

use event_loop::{
    DisconnectTransmiter, RequestTransmiter, ResponseReceiver, ResponseTransmiter,
    WebSocketEventLoop,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatus {
    SubscriptionPending,
    Subscribed,
    UnsubscriptionPending,
}

pub struct LnmWebSocketRepo {
    config: WebSocketApiConfig,
    event_loop_handle: SyncMutex<Option<JoinHandle<Result<()>>>>,
    disconnect_tx: DisconnectTransmiter,
    request_tx: RequestTransmiter,
    response_tx: ResponseTransmiter,
    connection_state_manager: Arc<ConnectionStateManager>,
    subscriptions: AsyncMutex<HashMap<LnmWebSocketChannel, ChannelStatus>>,
}

impl LnmWebSocketRepo {
    pub async fn new(config: WebSocketApiConfig, api_domain: String) -> Result<Arc<Self>> {
        // Internal channel for disconnect signal
        let (disconnect_tx, disconnect_rx) = mpsc::channel::<()>(1);

        // Internal channel for JSON RPC requests
        let (request_tx, request_rx) =
            mpsc::channel::<(LnmJsonRpcRequest, oneshot::Sender<bool>)>(100);

        // External channel for API responses
        let (response_tx, _) = broadcast::channel::<WebSocketApiRes>(100);

        let (event_loop_handle, connection_state_manager) = WebSocketEventLoop::try_spawn(
            api_domain,
            disconnect_rx,
            request_rx,
            response_tx.clone(),
        )
        .await?;

        Ok(Arc::new(Self {
            config,
            event_loop_handle: SyncMutex::new(Some(event_loop_handle)),
            disconnect_tx,
            request_tx,
            response_tx,
            connection_state_manager,
            subscriptions: AsyncMutex::new(HashMap::new()),
        }))
    }

    async fn evaluate_connection_status(&self) -> Result<()> {
        let connection_state = self.connection_state_manager.snapshot();

        if matches!(connection_state.as_ref(), ConnectionState::Connected) {
            return Ok(());
        }

        Err(WebSocketApiError::BadConnectionState(connection_state))
    }

    fn try_consume_event_loop_handle(&self) -> Option<JoinHandle<Result<()>>> {
        let mut handle_guard = self
            .event_loop_handle
            .lock()
            .expect("`event_loop_handle` mutex can't be poisoned");
        handle_guard.take()
    }
}

#[async_trait]
impl WebSocketRepository for LnmWebSocketRepo {
    async fn is_connected(&self) -> bool {
        self.connection_state_manager.is_connected()
    }

    async fn connection_state(&self) -> Arc<ConnectionState> {
        self.connection_state_manager.snapshot()
    }

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()> {
        self.evaluate_connection_status().await?;

        let channels: HashSet<LnmWebSocketChannel> = channels.into_iter().collect();
        if channels.is_empty() {
            return Ok(());
        }

        // Check current subscriptions
        let mut subscriptions_lock = self.subscriptions.lock().await;
        let mut channels_to_subscribe = Vec::new();

        for channel in channels {
            match subscriptions_lock.get(&channel) {
                Some(ChannelStatus::Subscribed | ChannelStatus::SubscriptionPending) => {
                    continue;
                }
                Some(ChannelStatus::UnsubscriptionPending) => {
                    return Err(WebSocketApiError::SubscribeWithUnsubscriptionPending(
                        channel,
                    ));
                }
                None => {
                    channels_to_subscribe.push(channel.clone());
                    subscriptions_lock.insert(channel, ChannelStatus::SubscriptionPending);
                }
            }
        }

        drop(subscriptions_lock);

        if channels_to_subscribe.is_empty() {
            return Ok(());
        }

        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        let req = LnmJsonRpcRequest::new(
            LnmJsonRpcReqMethod::Subscribe,
            channels_to_subscribe.clone(),
        );

        // Send subscription request to the manager task
        self.request_tx
            .send((req, oneshot_tx))
            .await
            .map_err(WebSocketApiError::SendSubscriptionRequest)?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(WebSocketApiError::ReceiveSubscriptionConfirmation)?;

        let mut subscriptions_lock = self.subscriptions.lock().await;

        for channel in channels_to_subscribe {
            let channel_status = subscriptions_lock.get(&channel).ok_or_else(|| {
                WebSocketApiError::InvalidSubscriptionsChannelNotFound(channel.clone())
            })?;

            if *channel_status != ChannelStatus::SubscriptionPending {
                return Err(WebSocketApiError::InvalidSubscriptionsChannelStatus {
                    channel: channel.clone(),
                    status: channel_status.clone(),
                });
            }

            if success {
                subscriptions_lock.insert(channel, ChannelStatus::Subscribed);
            } else {
                subscriptions_lock.remove(&channel);
            }
        }

        Ok(())
    }

    async fn unsubscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()> {
        self.evaluate_connection_status().await?;

        let channels: HashSet<LnmWebSocketChannel> = channels.into_iter().collect();
        if channels.is_empty() {
            return Ok(());
        }

        let mut subscriptions_lock = self.subscriptions.lock().await;
        let mut channels_to_unsubscribe = Vec::new();

        for channel in channels {
            match subscriptions_lock.get(&channel) {
                Some(ChannelStatus::Subscribed) => {
                    channels_to_unsubscribe.push(channel.clone());
                    subscriptions_lock.insert(channel, ChannelStatus::UnsubscriptionPending);
                }
                Some(ChannelStatus::SubscriptionPending) => {
                    return Err(WebSocketApiError::UnsubscribeWithSubscriptionPending(
                        channel,
                    ));
                }
                Some(ChannelStatus::UnsubscriptionPending) | None => {
                    continue;
                }
            }
        }

        drop(subscriptions_lock);

        if channels_to_unsubscribe.is_empty() {
            return Ok(());
        }

        let (oneshot_tx, oneshot_rx) = oneshot::channel();

        let req = LnmJsonRpcRequest::new(
            LnmJsonRpcReqMethod::Unsubscribe,
            channels_to_unsubscribe.clone(),
        );

        // Send unsubscription request to the manager task
        self.request_tx
            .send((req, oneshot_tx))
            .await
            .map_err(WebSocketApiError::SendUnubscriptionRequest)?;

        // Wait for confirmation
        let success = oneshot_rx
            .await
            .map_err(WebSocketApiError::ReceiveUnsubscriptionConfirmation)?;

        let mut subscriptions_lock = self.subscriptions.lock().await;

        for channel in channels_to_unsubscribe {
            let channel_status = subscriptions_lock.get(&channel).ok_or_else(|| {
                WebSocketApiError::InvalidSubscriptionsChannelNotFound(channel.clone())
            })?;

            if *channel_status != ChannelStatus::UnsubscriptionPending {
                return Err(WebSocketApiError::InvalidSubscriptionsChannelStatus {
                    channel: channel.clone(),
                    status: channel_status.clone(),
                });
            }

            if success {
                subscriptions_lock.remove(&channel);
            } else {
                subscriptions_lock.insert(channel, ChannelStatus::Subscribed);
            }
        }

        Ok(())
    }

    async fn subscriptions(&self) -> HashSet<LnmWebSocketChannel> {
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

    async fn receiver(&self) -> Result<ResponseReceiver> {
        self.evaluate_connection_status().await?;

        let broadcast_rx = self.response_tx.subscribe();
        Ok(broadcast_rx)
    }

    async fn disconnect(&self) -> Result<()> {
        if let Some(mut handle) = self.try_consume_event_loop_handle() {
            if handle.is_finished() {
                // The event loop task can only be finished due to errors or
                // via this `disconnect` method. The `connection_state_manager`
                // snapshot should reflect the termination condition.

                let e = match self.connection_state_manager.snapshot().as_ref() {
                    ConnectionState::Failed(e) => WebSocketApiError::Generic(format!(
                        "websocket connection was already failed with error {e}"
                    )),
                    _ => WebSocketApiError::Generic(
                        "event loop task terminated without proper connection state update"
                            .to_string(),
                    ),
                };

                return Err(e);
            }

            self.connection_state_manager
                .update(ConnectionState::DisconnectInitiated);

            let disconnect_send_res = self.disconnect_tx.send(()).await.map_err(|e| {
                handle.abort();
                WebSocketApiError::SendDisconnectRequest(e)
            });

            let disconnect_res = match disconnect_send_res {
                Ok(_) => {
                    tokio::select! {
                        join_res = &mut handle => {
                            join_res.map_err(WebSocketApiError::TaskJoin)?
                        }
                        _ = time::sleep(self.config.disconnect_timeout()) => {
                            handle.abort();
                            Err(WebSocketApiError::Generic("Disconnect timeout".to_string()))
                        }
                    }
                }
                Err(e) => Err(e),
            };

            return disconnect_res;
        }

        Err(WebSocketApiError::Generic(
            "websocket was already disconnected".to_string(),
        ))
    }
}

impl Drop for LnmWebSocketRepo {
    fn drop(&mut self) {
        if let Ok(mut handle) = self.event_loop_handle.lock() {
            if let Some(join_handle) = handle.take() {
                join_handle.abort();
            }
        }
    }
}
