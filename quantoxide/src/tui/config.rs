use std::time::Duration;

/// Configuration for terminal user interfaces.
#[derive(Clone, Debug)]
pub struct TuiConfig {
    event_check_interval: Duration,
    max_tui_log_len: usize,
    shutdown_timeout: Duration,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            event_check_interval: Duration::from_millis(50),
            max_tui_log_len: 10_000,
            shutdown_timeout: Duration::from_secs(6),
        }
    }
}

impl TuiConfig {
    /// Returns the interval for checking terminal events.
    pub fn event_check_interval(&self) -> Duration {
        self.event_check_interval
    }

    /// Returns the maximum number of log entries to retain in the TUI log buffer.
    pub fn max_tui_log_len(&self) -> usize {
        self.max_tui_log_len
    }

    /// Returns the timeout duration for graceful shutdown operations.
    pub fn shutdown_timeout(&self) -> Duration {
        self.shutdown_timeout
    }

    /// Sets the interval for checking terminal events.
    ///
    /// Default: `50` milliseconds
    pub fn with_event_check_interval(mut self, millis: u64) -> Self {
        self.event_check_interval = Duration::from_millis(millis);
        self
    }

    /// Sets the maximum number of log entries to retain in the TUI log buffer.
    ///
    /// Default: `10000`
    pub fn with_max_tui_log_len(mut self, len: usize) -> Self {
        self.max_tui_log_len = len;
        self
    }

    /// Sets the timeout duration for graceful shutdown operations.
    ///
    /// Default: `6` seconds
    pub fn with_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = Duration::from_secs(secs);
        self
    }
}
