use tokio::time;

use crate::trade::LiveConfig;

/// Configuration for the [`LiveSignalEngine`].
#[derive(Clone, Debug)]
pub struct LiveSignalConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveSignalConfig {
    fn default() -> Self {
        Self {
            sync_update_timeout: time::Duration::from_secs(5),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveSignalConfig {
    /// Returns the timeout duration for waiting on sync status updates.
    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    /// Returns the interval for restarting the signal evaluation process after recoverable errors.
    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    /// Returns the timeout duration for graceful shutdown operations.
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    /// Sets the timeout duration for waiting on sync status updates.
    ///
    /// Default: `5` seconds
    pub fn with_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    /// Sets the interval for restarting the signal evaluation process after recoverable errors.
    ///
    /// Default: `10` seconds
    pub fn with_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the timeout duration for graceful shutdown operations.
    ///
    /// Default: `6` seconds
    pub fn with_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

impl From<&LiveConfig> for LiveSignalConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

#[derive(Debug)]
pub(super) struct LiveSignalControllerConfig {
    shutdown_timeout: time::Duration,
}

impl LiveSignalControllerConfig {
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }
}

impl From<&LiveSignalConfig> for LiveSignalControllerConfig {
    fn from(value: &LiveSignalConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LiveSignalProcessConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
}

impl LiveSignalProcessConfig {
    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }
}

impl From<&LiveSignalConfig> for LiveSignalProcessConfig {
    fn from(value: &LiveSignalConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
        }
    }
}
