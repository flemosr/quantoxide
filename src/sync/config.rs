use std::num::NonZeroU64;

use chrono::{DateTime, Duration, Utc};
use tokio::time;

use lnm_sdk::{api_v2::WebSocketClientConfig, api_v3::RestClientConfig};

use crate::{trade::LiveTradeConfig, util::DateTimeExt};

use super::process::{
    sync_funding_settlements_task::LNM_SETTLEMENT_A_START,
    sync_price_history_task::LNM_OHLC_CANDLE_START,
};

/// Configuration for the synchronization engine.
#[derive(Clone, Debug)]
pub struct SyncConfig {
    rest_api_timeout: time::Duration,
    ws_api_disconnect_timeout: time::Duration,
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    price_history_reach: DateTime<Utc>,
    funding_settlement_reach: DateTime<Utc>,
    price_history_re_sync_interval: time::Duration,
    price_history_flag_gap_range: Option<Duration>,
    funding_settlement_flag_missing_range: Option<Duration>,
    live_price_tick_max_interval: time::Duration,
    funding_settlement_retry_interval: time::Duration,
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
            price_history_reach: (Utc::now() - Duration::days(90)).floor_day(),
            funding_settlement_reach: (Utc::now() - Duration::days(90))
                .floor_funding_settlement_time(),
            price_history_re_sync_interval: time::Duration::from_secs(10),
            price_history_flag_gap_range: Some(Duration::weeks(4)),
            funding_settlement_flag_missing_range: Some(Duration::weeks(4)),
            live_price_tick_max_interval: time::Duration::from_mins(3),
            funding_settlement_retry_interval: time::Duration::from_mins(1),
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
    pub fn price_history_reach(&self) -> DateTime<Utc> {
        self.price_history_reach
    }

    /// Returns how far back in time to fetch funding settlement data.
    pub fn funding_settlement_reach(&self) -> DateTime<Utc> {
        self.funding_settlement_reach
    }

    /// Returns the interval for re-synchronizing price history data.
    pub fn price_history_re_sync_interval(&self) -> time::Duration {
        self.price_history_re_sync_interval
    }

    /// Returns the time range (looking back from the current time) that will be scanned for gaps
    /// in the candle history during each backfill cycle.
    ///
    /// Only candles with `time >= now - range` will be analyzed for gaps.
    pub fn price_history_flag_gap_range(&self) -> Option<Duration> {
        self.price_history_flag_gap_range
    }

    /// Returns the time range (looking back from the current time) that will be scanned for missing
    /// funding settlements during each backfill cycle.
    pub fn funding_settlement_flag_missing_range(&self) -> Option<Duration> {
        self.funding_settlement_flag_missing_range
    }

    /// Returns the maximum interval between live price ticks before considering the connection
    /// stale.
    pub fn live_price_tick_max_interval(&self) -> time::Duration {
        self.live_price_tick_max_interval
    }

    /// Returns the retry interval for funding settlement sync when not yet caught up.
    pub fn funding_settlement_retry_interval(&self) -> time::Duration {
        self.funding_settlement_retry_interval
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
    /// The given time is floored to the start of the day (midnight UTC).
    ///
    /// Default: `Utc::now() - 90 days` (floored)
    pub fn with_price_history_reach(mut self, reach: DateTime<Utc>) -> Self {
        self.price_history_reach = reach.floor_day();
        self
    }

    /// Sets the price history reach to [`LNM_OHLC_CANDLE_START`], fetching the full available
    /// history.
    pub fn with_price_history_reach_max(mut self) -> Self {
        self.price_history_reach = LNM_OHLC_CANDLE_START;
        self
    }

    /// Sets how far back in time to fetch funding settlement data.
    ///
    /// The given time is floored to the previous valid funding settlement time.
    ///
    /// Default: `Utc::now() - 90 days` (floored)
    pub fn with_funding_settlement_reach(mut self, reach: DateTime<Utc>) -> Self {
        self.funding_settlement_reach = reach.floor_funding_settlement_time();
        self
    }

    /// Sets the funding settlement reach to [`LNM_SETTLEMENT_A_START`], fetching the full
    /// available history.
    pub fn with_funding_settlement_reach_max(mut self) -> Self {
        self.funding_settlement_reach = LNM_SETTLEMENT_A_START;
        self
    }

    /// Sets the interval for re-synchronizing price history data.
    ///
    /// Default: `10` seconds
    pub fn with_price_history_re_sync_interval(mut self, secs: u64) -> Self {
        self.price_history_re_sync_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the time range (looking back from the current time) to scan for gaps in the candle
    /// history during each backfill cycle.
    ///
    /// Only candles with `time >= now - range` will be analyzed for gaps.
    ///
    /// Default: `672` hours (4 weeks)
    pub fn with_price_history_flag_gap_range(mut self, hours: Option<u64>) -> Self {
        self.price_history_flag_gap_range = hours.map(|h| Duration::hours(h as i64));
        self
    }

    /// Sets the time range (looking back from the current time) to scan for missing funding
    /// settlements during each backfill cycle.
    ///
    /// Default: `672` hours (4 weeks)
    pub fn with_funding_settlement_flag_missing_range(mut self, hours: Option<u64>) -> Self {
        self.funding_settlement_flag_missing_range = hours.map(|h| Duration::hours(h as i64));
        self
    }

    /// Sets the maximum interval between live price ticks before considering the connection stale.
    ///
    /// Default: `180` seconds (3 minutes)
    pub fn with_live_price_tick_max_interval(mut self, secs: u64) -> Self {
        self.live_price_tick_max_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the retry interval for funding settlement sync when not yet caught up.
    ///
    /// Default: `60` seconds (1 minute)
    pub fn with_funding_settlement_retry_interval(mut self, secs: u64) -> Self {
        self.funding_settlement_retry_interval = time::Duration::from_secs(secs);
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

impl From<&LiveTradeConfig> for SyncConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        SyncConfig {
            rest_api_timeout: value.rest_api_timeout(),
            ws_api_disconnect_timeout: value.ws_api_disconnect_timeout(),
            rest_api_cooldown: value.rest_api_cooldown(),
            rest_api_error_cooldown: value.rest_api_error_cooldown(),
            rest_api_error_max_trials: value.rest_api_error_max_trials(),
            price_history_batch_size: value.price_history_batch_size(),
            price_history_reach: value.price_history_reach(),
            funding_settlement_reach: value.funding_settlement_reach(),
            price_history_re_sync_interval: value.price_history_re_sync_interval(),
            price_history_flag_gap_range: value.price_history_flag_gap_range(),
            funding_settlement_flag_missing_range: value.funding_settlement_flag_missing_range(),
            live_price_tick_max_interval: value.live_price_tick_max_interval(),
            funding_settlement_retry_interval: value.funding_sync_retry_interval(),
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
    price_history_reach: DateTime<Utc>,
    funding_settlement_reach: DateTime<Utc>,
    price_history_re_sync_interval: time::Duration,
    price_history_flag_gap_range: Option<Duration>,
    funding_settlement_flag_missing_range: Option<Duration>,
    live_price_tick_max_interval: time::Duration,
    funding_settlement_retry_interval: time::Duration,
    restart_interval: time::Duration,
}

impl SyncProcessConfig {
    pub fn price_history_re_sync_interval(&self) -> time::Duration {
        self.price_history_re_sync_interval
    }

    pub fn price_history_flag_gap_range(&self) -> Option<Duration> {
        self.price_history_flag_gap_range
    }

    pub fn funding_settlement_flag_missing_range(&self) -> Option<Duration> {
        self.funding_settlement_flag_missing_range
    }

    pub fn live_price_tick_max_interval(&self) -> time::Duration {
        self.live_price_tick_max_interval
    }

    pub fn funding_settlement_retry_interval(&self) -> time::Duration {
        self.funding_settlement_retry_interval
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
            funding_settlement_reach: value.funding_settlement_reach,
            price_history_re_sync_interval: value.price_history_re_sync_interval,
            price_history_flag_gap_range: value.price_history_flag_gap_range,
            funding_settlement_flag_missing_range: value.funding_settlement_flag_missing_range,
            live_price_tick_max_interval: value.live_price_tick_max_interval,
            funding_settlement_retry_interval: value.funding_settlement_retry_interval,
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
    price_history_reach: DateTime<Utc>,
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

    pub fn price_history_reach(&self) -> DateTime<Utc> {
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

#[derive(Clone)]
pub(crate) struct SyncFundingSettlementsTaskConfig {
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    funding_settlement_reach: DateTime<Utc>,
}

impl SyncFundingSettlementsTaskConfig {
    pub fn rest_api_cooldown(&self) -> time::Duration {
        self.rest_api_cooldown
    }

    pub fn rest_api_error_cooldown(&self) -> time::Duration {
        self.rest_api_error_cooldown
    }

    pub fn rest_api_error_max_trials(&self) -> NonZeroU64 {
        self.rest_api_error_max_trials
    }

    pub fn funding_settlement_reach(&self) -> DateTime<Utc> {
        self.funding_settlement_reach
    }
}

impl From<&SyncProcessConfig> for SyncFundingSettlementsTaskConfig {
    fn from(value: &SyncProcessConfig) -> Self {
        Self {
            rest_api_cooldown: value.rest_api_cooldown,
            rest_api_error_cooldown: value.rest_api_error_cooldown,
            rest_api_error_max_trials: value.rest_api_error_max_trials,
            funding_settlement_reach: value.funding_settlement_reach,
        }
    }
}
