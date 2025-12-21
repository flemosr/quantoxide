use std::time::Duration;

/// Configuration for REST API clients.
#[derive(Clone, Debug)]
pub struct RestClientConfig {
    timeout: Duration,
}

impl RestClientConfig {
    /// Creates a new REST client configuration with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Returns the request timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Sets the request timeout duration.
    ///
    /// Default: `20` seconds
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

/// Configuration for WebSocket clients.
#[derive(Clone, Debug)]
pub struct WebSocketClientConfig {
    disconnect_timeout: Duration,
}

impl WebSocketClientConfig {
    /// Creates a new WebSocket client configuration with the specified disconnect timeout.
    pub fn new(disconnect_timeout: Duration) -> Self {
        Self { disconnect_timeout }
    }

    /// Returns the disconnect timeout duration.
    pub fn disconnect_timeout(&self) -> Duration {
        self.disconnect_timeout
    }

    /// Sets the disconnect timeout duration.
    ///
    /// Default: `6` seconds
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
