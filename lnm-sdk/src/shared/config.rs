use std::time::Duration;

/// Configuration for LNM's [`ApiClient`].
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use lnm_sdk::api_v3::ApiClientConfig;
///
/// // Use default configuration
/// let config = ApiClientConfig::default();
///
/// // Customize configuration
/// let config = ApiClientConfig::default()
///     .with_rest_timeout(Duration::from_secs(20))
///     .with_ws_disconnect_timeout(Duration::from_secs(6));
/// ```
#[derive(Clone, Debug)]
pub struct ApiClientConfig {
    rest_timeout: Duration,
    ws_disconnect_timeout: Duration,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            rest_timeout: Duration::from_secs(20),
            ws_disconnect_timeout: Duration::from_secs(6),
        }
    }
}

impl ApiClientConfig {
    /// Returns the configured timeout for REST API requests.
    pub fn rest_timeout(&self) -> Duration {
        self.rest_timeout
    }

    /// Returns the configured timeout for WebSocket disconnect operations.
    pub fn ws_disconnect_timeout(&self) -> Duration {
        self.ws_disconnect_timeout
    }

    /// Sets the REST API request timeout. The default is 20 seconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use lnm_sdk::api_v3::ApiClientConfig;
    ///
    /// let config = ApiClientConfig::default()
    ///     .with_rest_timeout(Duration::from_secs(20));
    /// ```
    pub fn with_rest_timeout(mut self, timeout: Duration) -> Self {
        self.rest_timeout = timeout;
        self
    }

    /// Sets the WebSocket disconnect timeout. The default is 6 seconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use lnm_sdk::api_v3::ApiClientConfig;
    ///
    /// let config = ApiClientConfig::default()
    ///     .with_ws_disconnect_timeout(Duration::from_secs(6));
    /// ```
    pub fn with_ws_disconnect_timeout(mut self, timeout: Duration) -> Self {
        self.ws_disconnect_timeout = timeout;
        self
    }
}

#[derive(Clone, Debug)]
pub struct RestClientConfig {
    timeout: Duration,
}

impl RestClientConfig {
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl From<&ApiClientConfig> for RestClientConfig {
    fn from(value: &ApiClientConfig) -> Self {
        Self {
            timeout: value.rest_timeout(),
        }
    }
}

impl Default for RestClientConfig {
    fn default() -> Self {
        (&ApiClientConfig::default()).into()
    }
}

#[derive(Clone, Debug)]
pub struct WebSocketClientConfig {
    disconnect_timeout: Duration,
}

impl WebSocketClientConfig {
    pub fn disconnect_timeout(&self) -> Duration {
        self.disconnect_timeout
    }

    pub fn with_disconnect_timeout(mut self, disconnect_timeout: Duration) -> Self {
        self.disconnect_timeout = disconnect_timeout;
        self
    }
}

impl From<&ApiClientConfig> for WebSocketClientConfig {
    fn from(value: &ApiClientConfig) -> Self {
        Self {
            disconnect_timeout: value.ws_disconnect_timeout(),
        }
    }
}

impl Default for WebSocketClientConfig {
    fn default() -> Self {
        (&ApiClientConfig::default()).into()
    }
}
