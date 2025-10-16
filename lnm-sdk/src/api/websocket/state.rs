use std::sync::{Arc, Mutex, MutexGuard};

use super::error::WebSocketApiError;

#[derive(Debug)]
pub enum ConnectionStatus {
    Connected,
    DisconnectInitiated,
    Disconnected,
    Failed(WebSocketApiError),
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected)
    }
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionStatus::Connected => write!(f, "Connected"),
            ConnectionStatus::DisconnectInitiated => write!(f, "Disconnect Initiated"),
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Failed(err) => write!(f, "Failed: {}", err),
        }
    }
}

pub struct ConnectionStatusManager(Mutex<Arc<ConnectionStatus>>);

impl ConnectionStatusManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Arc::new(ConnectionStatus::Connected))))
    }

    fn lock_status(&self) -> MutexGuard<'_, Arc<ConnectionStatus>> {
        self.0
            .lock()
            .expect("`ConnectionStatusManager` mutex can't be poisoned")
    }

    pub fn update(&self, new_status: ConnectionStatus) {
        let mut status_guard = self.lock_status();

        *status_guard = Arc::new(new_status)
    }

    pub fn snapshot(&self) -> Arc<ConnectionStatus> {
        self.lock_status().clone()
    }

    pub fn is_connected(&self) -> bool {
        self.snapshot().is_connected()
    }
}
