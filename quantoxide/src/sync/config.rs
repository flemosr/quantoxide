use std::num::NonZeroU64;

use chrono::Duration;
use tokio::time;

use lnm_sdk::{api_v2::WebSocketClientConfig, api_v3::RestClientConfig};

use crate::trade::LiveConfig;

/// Configuration for the synchronization engine.
#[derive(Clone, Debug)]
pub struct SyncConfig {
    rest_api_timeout: time::Duration,
    ws_api_disconnect_timeout: time::Duration,
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    price_history_reach: Duration,
    price_history_re_sync_interval: time::Duration,
    live_price_tick_max_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            rest_api_timeout: time::Duration::from_secs(20),
            ws_api_disconnect_timeout: time::Duration::from_secs(6),
            rest_api_cooldown: time::Duration::from_secs(2),
            rest_api_error_cooldown: time::Duration::from_secs(10),
            rest_api_error_max_trials: 3.try_into().expect("not zero"),
            price_history_batch_size: 1000.try_into().expect("not zero"),
            price_history_reach: Duration::days(90),
            price_history_re_sync_interval: time::Duration::from_secs(10),
            live_price_tick_max_interval: time::Duration::from_mins(3),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl SyncConfig {
    /// Returns the timeout duration for REST API requests.
    pub fn rest_api_timeout(&self) -> time::Duration {
        self.rest_api_timeout
    }

    /// Returns the disconnect timeout for WebSocket API connections.
    pub fn ws_api_disconnect_timeout(&self) -> time::Duration {
        self.ws_api_disconnect_timeout
    }

    /// Returns the cooldown period between REST API requests.
    pub fn rest_api_cooldown(&self) -> time::Duration {
        self.rest_api_cooldown
    }

    /// Returns the cooldown period after REST API errors before retrying.
    pub fn rest_api_error_cooldown(&self) -> time::Duration {
        self.rest_api_error_cooldown
    }

    /// Returns the maximum number of retry attempts for REST API errors.
    pub fn rest_api_error_max_trials(&self) -> NonZeroU64 {
        self.rest_api_error_max_trials
    }

    /// Returns the batch size for fetching price history data.
    pub fn price_history_batch_size(&self) -> NonZeroU64 {
        self.price_history_batch_size
    }

    /// Returns how far back in time to fetch price history data.
    pub fn price_history_reach(&self) -> Duration {
        self.price_history_reach
    }

    /// Returns the interval for re-synchronizing price history data.
    pub fn price_history_re_sync_interval(&self) -> time::Duration {
        self.price_history_re_sync_interval
    }

    /// Returns the maximum interval between live price ticks before considering the connection
    /// stale.
    pub fn live_price_tick_max_interval(&self) -> time::Duration {
        self.live_price_tick_max_interval
    }

    /// Returns the interval for restarting the synchronization process.
    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    /// Returns the timeout duration for graceful shutdown operations.
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    /// Sets the timeout duration for REST API requests.
    ///
    /// Default: `20` seconds
    pub fn with_rest_api_timeout(mut self, secs: u64) -> Self {
        self.rest_api_timeout = time::Duration::from_secs(secs);
        self
    }

    /// Sets the disconnect timeout for WebSocket API connections.
    ///
    /// Default: `6` seconds
    pub fn with_ws_api_disconnect_timeout(mut self, secs: u64) -> Self {
        self.ws_api_disconnect_timeout = time::Duration::from_secs(secs);
        self
    }

    /// Sets the cooldown period between REST API requests.
    ///
    /// Default: `2` seconds
    pub fn with_rest_api_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_cooldown = time::Duration::from_secs(secs);
        self
    }

    /// Sets the cooldown period after REST API errors before retrying.
    ///
    /// Default: `10` seconds
    pub fn with_rest_api_error_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    /// Sets the maximum number of retry attempts for REST API errors.
    ///
    /// Default: `3`
    pub fn with_rest_api_error_max_trials(mut self, max_trials: NonZeroU64) -> Self {
        self.rest_api_error_max_trials = max_trials;
        self
    }

    /// Sets the batch size for fetching price history data.
    ///
    /// Default: `1000`
    pub fn with_price_history_batch_size(mut self, size: NonZeroU64) -> Self {
        self.price_history_batch_size = size;
        self
    }

    /// Sets how far back in time to fetch price history data.
    ///
    /// Default: `90` days
    pub fn with_price_history_reach(mut self, days: NonZeroU64) -> Self {
        self.price_history_reach = Duration::days(days.get() as i64);
        self
    }

    /// Sets the interval for re-synchronizing price history data.
    ///
    /// Default: `10` seconds
    pub fn with_price_history_re_sync_interval(mut self, secs: u64) -> Self {
        self.price_history_re_sync_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the maximum interval between live price ticks before considering the connection stale.
    ///
    /// Default: `180` seconds (3 minutes)
    pub fn with_live_price_tick_max_interval(mut self, secs: u64) -> Self {
        self.live_price_tick_max_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the interval for restarting the synchronization process.
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

impl From<&SyncConfig> for RestClientConfig {
    fn from(value: &SyncConfig) -> Self {
        RestClientConfig::new(value.rest_api_timeout())
    }
}

impl From<&SyncConfig> for WebSocketClientConfig {
    fn from(value: &SyncConfig) -> Self {
        WebSocketClientConfig::new(value.ws_api_disconnect_timeout())
    }
}

impl From<&LiveConfig> for SyncConfig {
    fn from(value: &LiveConfig) -> Self {
        SyncConfig {
            rest_api_timeout: value.rest_api_timeout(),
            ws_api_disconnect_timeout: value.ws_api_disconnect_timeout(),
            rest_api_cooldown: value.rest_api_cooldown(),
            rest_api_error_cooldown: value.rest_api_error_cooldown(),
            rest_api_error_max_trials: value.rest_api_error_max_trials(),
            price_history_batch_size: value.price_history_batch_size(),
            price_history_reach: value.price_history_reach(),
            price_history_re_sync_interval: value.price_history_re_sync_interval(),
            live_price_tick_max_interval: value.live_price_tick_max_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SyncControllerConfig {
    shutdown_timeout: time::Duration,
}

impl SyncControllerConfig {
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }
}

impl From<&SyncConfig> for SyncControllerConfig {
    fn from(value: &SyncConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SyncProcessConfig {
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    price_history_reach: Duration,
    price_history_re_sync_interval: time::Duration,
    live_price_tick_max_interval: time::Duration,
    restart_interval: time::Duration,
}

impl SyncProcessConfig {
    pub fn price_history_re_sync_interval(&self) -> time::Duration {
        self.price_history_re_sync_interval
    }

    pub fn live_price_tick_max_interval(&self) -> time::Duration {
        self.live_price_tick_max_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }
}

impl From<&SyncConfig> for SyncProcessConfig {
    fn from(value: &SyncConfig) -> Self {
        Self {
            rest_api_cooldown: value.rest_api_cooldown,
            rest_api_error_cooldown: value.rest_api_error_cooldown,
            rest_api_error_max_trials: value.rest_api_error_max_trials,
            price_history_batch_size: value.price_history_batch_size,
            price_history_reach: value.price_history_reach,
            price_history_re_sync_interval: value.price_history_re_sync_interval,
            live_price_tick_max_interval: value.live_price_tick_max_interval,
            restart_interval: value.restart_interval,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SyncPriceHistoryTaskConfig {
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    price_history_reach: Duration,
}

impl SyncPriceHistoryTaskConfig {
    pub fn rest_api_cooldown(&self) -> time::Duration {
        self.rest_api_cooldown
    }

    pub fn rest_api_error_cooldown(&self) -> time::Duration {
        self.rest_api_error_cooldown
    }

    pub fn rest_api_error_max_trials(&self) -> NonZeroU64 {
        self.rest_api_error_max_trials
    }

    pub fn price_history_batch_size(&self) -> NonZeroU64 {
        self.price_history_batch_size
    }

    pub fn price_history_reach(&self) -> Duration {
        self.price_history_reach
    }
}

impl From<&SyncProcessConfig> for SyncPriceHistoryTaskConfig {
    fn from(value: &SyncProcessConfig) -> Self {
        Self {
            rest_api_cooldown: value.rest_api_cooldown,
            rest_api_error_cooldown: value.rest_api_error_cooldown,
            rest_api_error_max_trials: value.rest_api_error_max_trials,
            price_history_batch_size: value.price_history_batch_size,
            price_history_reach: value.price_history_reach,
        }
    }
}
