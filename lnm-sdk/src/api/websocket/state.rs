use std::sync::{Arc, Mutex};

use super::error::WebSocketApiError;

#[derive(Debug)]
pub enum ConnectionState {
    Connected,
    DisconnectInitiated,
    Disconnected,
    Failed(WebSocketApiError),
}

pub struct ConnectionStateManager(Mutex<Arc<ConnectionState>>);

impl ConnectionStateManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Arc::new(ConnectionState::Connected))))
    }

    pub fn update(&self, new_state: ConnectionState) {
        let mut state_guard = self
            .0
            .lock()
            .expect("`ConnectionStateManager` mutex can't be poisoned");

        *state_guard = Arc::new(new_state)
    }

    pub fn snapshot(&self) -> Arc<ConnectionState> {
        self.0
            .lock()
            .expect("`ConnectionStateManager` mutex can't be poisoned")
            .clone()
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.snapshot().as_ref(), ConnectionState::Connected)
    }
}
