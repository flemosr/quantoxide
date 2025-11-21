use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RestClientConfig {
    timeout: Duration,
}

impl RestClientConfig {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for RestClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WebSocketClientConfig {
    disconnect_timeout: Duration,
}

impl WebSocketClientConfig {
    pub fn new(disconnect_timeout: Duration) -> Self {
        Self { disconnect_timeout }
    }

    pub fn disconnect_timeout(&self) -> Duration {
        self.disconnect_timeout
    }

    pub fn with_disconnect_timeout(mut self, disconnect_timeout: Duration) -> Self {
        self.disconnect_timeout = disconnect_timeout;
        self
    }
}

impl Default for WebSocketClientConfig {
    fn default() -> Self {
        Self {
            disconnect_timeout: Duration::from_secs(6),
        }
    }
}
