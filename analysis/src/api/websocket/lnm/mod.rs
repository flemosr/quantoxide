use async_trait::async_trait;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    sync::{oneshot, Mutex},
    task::JoinHandle,
};

use super::models::{LnmJsonRpcReqMethod, LnmJsonRpcRequest, LnmWebSocketChannel};
use super::repositories::WebSocketRepository;
use super::{
    error::{Result, WebSocketApiError},
    models::ConnectionState,
};

mod manager;

use manager::{
    ManagerTask, RequestTransmiter, ResponseReceiver, ResponseTransmiter, ShutdownTransmiter,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChannelStatus {
    SubscriptionPending,
    Subscribed,
    UnsubscriptionPending,
}

pub struct LnmWebSocketRepo {
    manager_task_handle: JoinHandle<Result<()>>,
    shutdown_tx: ShutdownTransmiter,
    requests_tx: RequestTransmiter,
    responses_tx: ResponseTransmiter,
    connection_state: Arc<Mutex<ConnectionState>>,
    subscriptions: Arc<Mutex<HashMap<LnmWebSocketChannel, ChannelStatus>>>,
}

impl LnmWebSocketRepo {
    pub async fn new(api_domain: String) -> Result<Self> {
        let (manager_task, shutdown_tx, requests_tx, responses_tx, connection_state) =
            ManagerTask::new(api_domain).await?;

        let manager_task_handle = tokio::spawn(manager_task.run());

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));

        Ok(Self {
            manager_task_handle,
            connection_state,
            shutdown_tx,
            requests_tx,
            responses_tx,
            subscriptions,
        })
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
}

#[async_trait]
impl WebSocketRepository for LnmWebSocketRepo {
    fn is_connected(&self) -> bool {
        !self.manager_task_handle.is_finished()
    }

    async fn connection_state(&self) -> ConnectionState {
        self.connection_state.lock().await.clone()
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
                    return Err(WebSocketApiError::Generic(format!(
                        "Channel {channel} is pending unsubscription"
                    )));
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

    async fn shutdown(&self) -> Result<()> {
        if self.manager_task_handle.is_finished() {
            return self.evaluate_manager_status().await;
        }

        self.shutdown_tx
            .send(())
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        Ok(())
    }
}
