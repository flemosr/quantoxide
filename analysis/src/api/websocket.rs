use connection::WebSocketApiConnection;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex},
    task::JoinHandle,
};

mod connection;
pub mod error;
mod manager;
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
    shutdown_tx: mpsc::Sender<()>, // select! doesn't handle oneshot well
    requests_tx: mpsc::Sender<(LnmJsonRpcRequest, oneshot::Sender<bool>)>,
    responses_tx: broadcast::Sender<WebSocketApiRes>,
    connection_state: Arc<Mutex<ConnectionState>>,
    subscriptions: Arc<Mutex<HashMap<LnmWebSocketChannel, ChannelStatus>>>,
}

impl WebSocketAPI {
    pub async fn new(api_domain: String) -> Result<Self> {
        let ws = WebSocketApiConnection::new(api_domain).await?;

        // Internal channel for shutdown signal
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Internal channel for JSON RPC requests
        let (requests_tx, resquests_rx) =
            mpsc::channel::<(LnmJsonRpcRequest, oneshot::Sender<bool>)>(100);

        // External channel for API responses
        let (responses_tx, _) = broadcast::channel::<WebSocketApiRes>(100);

        let connection_state = Arc::new(Mutex::new(ConnectionState::Connected));

        let manager_handle = tokio::spawn(manager::task(
            ws,
            shutdown_rx,
            resquests_rx,
            responses_tx.clone(),
            connection_state.clone(),
        ));

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));

        Ok(WebSocketAPI {
            manager_handle,
            connection_state,
            shutdown_tx,
            requests_tx,
            responses_tx,
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

        let req = LnmJsonRpcRequest::new(
            LnmJsonRpcReqMethod::Unsubscribe,
            channels_to_unsubscribe.clone(),
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

        let broadcast_rx = self.responses_tx.subscribe();
        Ok(broadcast_rx)
    }

    pub async fn shutdown(self) -> Result<()> {
        if !self.manager_handle.is_finished() {
            self.shutdown_tx
                .send(())
                .await
                .map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        }

        self.manager_handle
            .await
            .map_err(|e| WebSocketApiError::Generic(e.to_string()))?
    }
}
