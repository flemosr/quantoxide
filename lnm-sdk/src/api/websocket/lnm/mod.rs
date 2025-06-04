use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as SyncMutex},
};

use async_trait::async_trait;
use tokio::{
    sync::{Mutex as AsyncMutex, oneshot},
    task::JoinHandle,
    time,
};

use super::repositories::WebSocketRepository;
use super::{
    WebSocketApiConfig,
    models::{LnmJsonRpcReqMethod, LnmJsonRpcRequest, LnmWebSocketChannel},
};
use super::{
    error::{Result, WebSocketApiError},
    models::ConnectionState,
};

mod event_loop;

use event_loop::{
    DisconnectTransmiter, ManagerTask, RequestTransmiter, ResponseReceiver, ResponseTransmiter,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatus {
    SubscriptionPending,
    Subscribed,
    UnsubscriptionPending,
}

pub struct LnmWebSocketRepo {
    config: WebSocketApiConfig,
    manager_handle: SyncMutex<Option<JoinHandle<Result<()>>>>,
    disconnect_tx: DisconnectTransmiter,
    requests_tx: RequestTransmiter,
    responses_tx: ResponseTransmiter,
    connection_state: Arc<AsyncMutex<Arc<ConnectionState>>>,
    subscriptions: Arc<AsyncMutex<HashMap<LnmWebSocketChannel, ChannelStatus>>>,
}

impl LnmWebSocketRepo {
    pub async fn new(config: WebSocketApiConfig, api_domain: String) -> Result<Arc<Self>> {
        let (manager_task, disconnect_tx, requests_tx, responses_tx, connection_state) =
            ManagerTask::new(api_domain).await?;

        let manager_handle = tokio::spawn(manager_task.run());

        let subscriptions = Arc::new(AsyncMutex::new(HashMap::new()));

        Ok(Arc::new(Self {
            config,
            manager_handle: SyncMutex::new(Some(manager_handle)),
            connection_state,
            disconnect_tx,
            requests_tx,
            responses_tx,
            subscriptions,
        }))
    }

    async fn evaluate_manager_status(&self) -> Result<()> {
        let connection_state = self.connection_state().await;
        match connection_state.as_ref() {
            ConnectionState::Connected => Ok(()),
            ConnectionState::Failed(_) | ConnectionState::Disconnected => {
                Err(WebSocketApiError::BadConnectionState(connection_state))
            }
        }
    }
}

#[async_trait]
impl WebSocketRepository for LnmWebSocketRepo {
    async fn is_connected(&self) -> bool {
        let handle_guard = self
            .manager_handle
            .lock()
            .expect("manager_handle mutex can't be poisoned");
        if let Some(handle) = handle_guard.as_ref() {
            return !handle.is_finished();
        }
        false
    }

    async fn connection_state(&self) -> Arc<ConnectionState> {
        let connection_state_guard = self.connection_state.lock().await;
        (*connection_state_guard).clone()
    }

    async fn subscribe(&self, channels: Vec<LnmWebSocketChannel>) -> Result<()> {
        self.evaluate_manager_status().await?;

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
        self.requests_tx
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
        self.evaluate_manager_status().await?;

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
        self.requests_tx
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
        self.evaluate_manager_status().await?;

        let broadcast_rx = self.responses_tx.subscribe();
        Ok(broadcast_rx)
    }

    async fn disconnect(&self) -> Result<()> {
        let handle_opt = {
            let mut handle_guard = self
                .manager_handle
                .lock()
                .expect("manager_handle mutex can't be poisoned");
            handle_guard.take()
        };

        if let Some(mut handle) = handle_opt {
            if handle.is_finished() {
                let ws_res = handle.await.map_err(WebSocketApiError::TaskJoin)?;
                if let Err(e) = ws_res {
                    return Err(WebSocketApiError::Generic(format!(
                        "websocket connection was already failed with error {e}"
                    )));
                }

                return Err(WebSocketApiError::Generic(
                    "websocket disconnected unexpectedly".to_string(),
                ));
            }

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

        return Err(WebSocketApiError::Generic(
            "websocket was already disconnected".to_string(),
        ));
    }
}

impl Drop for LnmWebSocketRepo {
    fn drop(&mut self) {
        if let Ok(mut handle) = self.manager_handle.lock() {
            if let Some(join_handle) = handle.take() {
                join_handle.abort();
            }
        }
    }
}
