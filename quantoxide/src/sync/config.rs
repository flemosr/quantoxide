use std::num::NonZeroU64;

use chrono::Duration;
use tokio::time;

use lnm_sdk::{api_v2::WebSocketClientConfig, api_v3::RestClientConfig};

use crate::trade::{LiveConfig, LiveTradeExecutorConfig};

#[derive(Clone, Debug)]
pub struct SyncConfig {
    api_rest_timeout: time::Duration,
    api_ws_disconnect_timeout: time::Duration,
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: NonZeroU64,
    api_history_batch_size: NonZeroU64,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    max_tick_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            api_rest_timeout: time::Duration::from_secs(20),
            api_ws_disconnect_timeout: time::Duration::from_secs(6),
            api_cooldown: time::Duration::from_secs(2),
            api_error_cooldown: time::Duration::from_secs(10),
            api_error_max_trials: 3.try_into().expect("not zero"),
            api_history_batch_size: 1000.try_into().expect("not zero"),
            sync_history_reach: Duration::days(90),
            re_sync_history_interval: time::Duration::from_secs(10),
            max_tick_interval: time::Duration::from_mins(3),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl SyncConfig {
    pub fn api_rest_timeout(&self) -> time::Duration {
        self.api_rest_timeout
    }

    pub fn api_ws_disconnect_timeout(&self) -> time::Duration {
        self.api_ws_disconnect_timeout
    }

    pub fn api_cooldown(&self) -> time::Duration {
        self.api_cooldown
    }

    pub fn api_error_cooldown(&self) -> time::Duration {
        self.api_error_cooldown
    }

    pub fn api_error_max_trials(&self) -> NonZeroU64 {
        self.api_error_max_trials
    }

    pub fn api_history_batch_size(&self) -> NonZeroU64 {
        self.api_history_batch_size
    }

    pub fn sync_history_reach(&self) -> Duration {
        self.sync_history_reach
    }

    pub fn re_sync_history_interval(&self) -> time::Duration {
        self.re_sync_history_interval
    }

    pub fn max_tick_interval(&self) -> time::Duration {
        self.max_tick_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn with_api_rest_timeout(mut self, secs: u64) -> Self {
        self.api_rest_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_ws_disconnect_timeout(mut self, secs: u64) -> Self {
        self.api_ws_disconnect_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_cooldown(mut self, secs: u64) -> Self {
        self.api_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_error_cooldown(mut self, secs: u64) -> Self {
        self.api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_error_max_trials(mut self, max_trials: NonZeroU64) -> Self {
        self.api_error_max_trials = max_trials;
        self
    }

    pub fn with_api_history_batch_size(mut self, size: NonZeroU64) -> Self {
        self.api_history_batch_size = size;
        self
    }

    pub fn with_sync_history_reach(mut self, days: NonZeroU64) -> Self {
        self.sync_history_reach = Duration::days(days.get() as i64);
        self
    }

    pub fn with_re_sync_history_interval(mut self, secs: u64) -> Self {
        self.re_sync_history_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn with_max_tick_interval(mut self, secs: u64) -> Self {
        self.max_tick_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn with_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn with_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

impl From<&SyncConfig> for RestClientConfig {
    fn from(value: &SyncConfig) -> Self {
        RestClientConfig::new(value.api_rest_timeout())
    }
}

impl From<&SyncConfig> for WebSocketClientConfig {
    fn from(value: &SyncConfig) -> Self {
        WebSocketClientConfig::new(value.api_ws_disconnect_timeout())
    }
}

impl From<&LiveConfig> for SyncConfig {
    fn from(value: &LiveConfig) -> Self {
        SyncConfig {
            api_rest_timeout: value.api_rest_timeout(),
            api_ws_disconnect_timeout: value.api_ws_disconnect_timeout(),
            api_cooldown: value.api_cooldown(),
            api_error_cooldown: value.api_error_cooldown(),
            api_error_max_trials: value.api_error_max_trials(),
            api_history_batch_size: value.api_history_batch_size(),
            sync_history_reach: value.sync_history_reach(),
            re_sync_history_interval: value.re_sync_history_interval(),
            max_tick_interval: value.max_tick_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

// Important: `SyncEngine` is only initialized by `LiveTradeExecutor` in 'live with no lookback'
// mode. Therefore, only the configs used while on this mode need to be customizable by consumers.
// Besides that, `WebSocketClientConfig` is obtained directly from `&LiveTradeExecutorConfig`, so
// the corresponding properties shouldn't be transfered here.
// Unnecessary `SyncConfig` properties are being set with defaults for the sake of simplicity.
impl From<&LiveTradeExecutorConfig> for SyncConfig {
    fn from(value: &LiveTradeExecutorConfig) -> Self {
        SyncConfig::default()
            .with_max_tick_interval(value.max_tick_interval().as_secs())
            .with_restart_interval(value.restart_interval().as_secs())
            .with_shutdown_timeout(value.shutdown_timeout().as_secs())
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
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: NonZeroU64,
    api_history_batch_size: NonZeroU64,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    max_tick_interval: time::Duration,
    restart_interval: time::Duration,
}

impl SyncProcessConfig {
    pub fn re_sync_history_interval(&self) -> time::Duration {
        self.re_sync_history_interval
    }

    pub fn max_tick_interval(&self) -> time::Duration {
        self.max_tick_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }
}

impl From<&SyncConfig> for SyncProcessConfig {
    fn from(value: &SyncConfig) -> Self {
        Self {
            api_cooldown: value.api_cooldown,
            api_error_cooldown: value.api_error_cooldown,
            api_error_max_trials: value.api_error_max_trials,
            api_history_batch_size: value.api_history_batch_size,
            sync_history_reach: value.sync_history_reach,
            re_sync_history_interval: value.re_sync_history_interval,
            max_tick_interval: value.max_tick_interval,
            restart_interval: value.restart_interval,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SyncPriceHistoryTaskConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: NonZeroU64,
    api_history_batch_size: NonZeroU64,
    sync_history_reach: Duration,
}

impl SyncPriceHistoryTaskConfig {
    pub fn api_cooldown(&self) -> time::Duration {
        self.api_cooldown
    }

    pub fn api_error_cooldown(&self) -> time::Duration {
        self.api_error_cooldown
    }

    pub fn api_error_max_trials(&self) -> NonZeroU64 {
        self.api_error_max_trials
    }

    pub fn api_history_batch_size(&self) -> NonZeroU64 {
        self.api_history_batch_size
    }

    pub fn sync_history_reach(&self) -> Duration {
        self.sync_history_reach
    }
}

impl From<&SyncProcessConfig> for SyncPriceHistoryTaskConfig {
    fn from(value: &SyncProcessConfig) -> Self {
        Self {
            api_cooldown: value.api_cooldown,
            api_error_cooldown: value.api_error_cooldown,
            api_error_max_trials: value.api_error_max_trials,
            api_history_batch_size: value.api_history_batch_size,
            sync_history_reach: value.sync_history_reach,
        }
    }
}
