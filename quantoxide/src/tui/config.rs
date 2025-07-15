use std::time::Duration;

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
    pub fn event_check_interval(&self) -> Duration {
        self.event_check_interval
    }

    pub fn max_tui_log_len(&self) -> usize {
        self.max_tui_log_len
    }

    pub fn shutdown_timeout(&self) -> Duration {
        self.shutdown_timeout
    }

    pub fn set_event_check_interval(mut self, millis: u64) -> Self {
        self.event_check_interval = Duration::from_millis(millis);
        self
    }

    pub fn set_max_tui_log_len(mut self, len: usize) -> Self {
        self.max_tui_log_len = len;
        self
    }

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = Duration::from_secs(secs);
        self
    }
}
