use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use super::error::WebSocketConnectionError;

#[derive(Debug)]
pub enum WsConnectionStatus {
    Connected,
    DisconnectInitiated,
    Disconnected,
    Failed(WebSocketConnectionError),
}

impl WsConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, WsConnectionStatus::Connected)
    }
}

impl fmt::Display for WsConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WsConnectionStatus::Connected => write!(f, "Connected"),
            WsConnectionStatus::DisconnectInitiated => write!(f, "Disconnect Initiated"),
            WsConnectionStatus::Disconnected => write!(f, "Disconnected"),
            WsConnectionStatus::Failed(err) => write!(f, "Failed: {}", err),
        }
    }
}

pub(super) struct WsConnectionStatusManager(Mutex<Arc<WsConnectionStatus>>);

impl WsConnectionStatusManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Arc::new(WsConnectionStatus::Connected))))
    }

    fn lock_status(&self) -> MutexGuard<'_, Arc<WsConnectionStatus>> {
        self.0
            .lock()
            .expect("`WsConnectionStatusManager` mutex can't be poisoned")
    }

    pub fn update(&self, new_status: WsConnectionStatus) {
        let mut status_guard = self.lock_status();

        *status_guard = Arc::new(new_status)
    }

    pub fn snapshot(&self) -> Arc<WsConnectionStatus> {
        self.lock_status().clone()
    }

    pub fn is_connected(&self) -> bool {
        self.lock_status().is_connected()
    }
}
