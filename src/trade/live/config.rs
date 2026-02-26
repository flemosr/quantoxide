use std::num::NonZeroU64;

use chrono::{DateTime, Duration, Utc};
use tokio::time;

use lnm_sdk::{
    api_v2::WebSocketClientConfig,
    api_v3::{RestClientConfig, models::PercentageCapped},
};

use crate::{
    sync::{LNM_OHLC_CANDLE_START, LNM_SETTLEMENT_A_START},
    util::DateTimeExt,
};

/// Configuration for the [`LiveTradeEngine`](crate::trade::LiveTradeEngine) controlling
/// synchronization, signal processing, trade execution, and session management.
#[derive(Clone, Debug)]
pub struct LiveTradeConfig {
    rest_api_timeout: time::Duration,
    ws_api_disconnect_timeout: time::Duration,
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    sync_mode_full: bool,
    price_history_reach: DateTime<Utc>,
    funding_settlement_reach: DateTime<Utc>,
    price_history_re_sync_interval: time::Duration,
    price_history_re_backfill_interval: time::Duration,
    price_history_flag_gap_range: Option<Duration>,
    funding_settlement_flag_missing_range: Option<Duration>,
    live_price_tick_max_interval: time::Duration,
    funding_sync_retry_interval: time::Duration,
    sync_update_timeout: time::Duration,
    trade_tsl_step_size: PercentageCapped,
    startup_clean_up_trades: bool,
    startup_recover_trades: bool,
    trading_session_refresh_interval: time::Duration,
    shutdown_clean_up_trades: bool,
    trade_estimated_fee: PercentageCapped,
    trade_max_running_qtd: usize,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveTradeConfig {
    fn default() -> Self {
        Self {
            rest_api_timeout: time::Duration::from_secs(20),
            ws_api_disconnect_timeout: time::Duration::from_secs(6),
            rest_api_cooldown: time::Duration::from_secs(2),
            rest_api_error_cooldown: time::Duration::from_secs(10),
            rest_api_error_max_trials: 3.try_into().expect("not zero"),
            price_history_batch_size: 1000.try_into().expect("not zero"),
            sync_mode_full: false,
            price_history_reach: (Utc::now() - Duration::days(90)).floor_day(),
            funding_settlement_reach: (Utc::now() - Duration::days(90))
                .floor_funding_settlement_time(),
            price_history_re_sync_interval: time::Duration::from_secs(10),
            price_history_re_backfill_interval: time::Duration::from_secs(90),
            price_history_flag_gap_range: Some(Duration::weeks(4)),
            funding_settlement_flag_missing_range: Some(Duration::weeks(4)),
            live_price_tick_max_interval: time::Duration::from_secs(3 * 60),
            funding_sync_retry_interval: time::Duration::from_secs(60),
            sync_update_timeout: time::Duration::from_secs(5),
            trade_tsl_step_size: PercentageCapped::MIN,
            startup_clean_up_trades: false,
            startup_recover_trades: true,
            trading_session_refresh_interval: time::Duration::from_millis(1_000),
            shutdown_clean_up_trades: false,
            trade_estimated_fee: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            trade_max_running_qtd: 50,
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveTradeConfig {
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

    /// Returns whether full synchronization mode is enabled (includes complete historical data).
    pub fn sync_mode_full(&self) -> bool {
        self.sync_mode_full
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

    /// Returns the interval for re-backfilling price history data in backfill mode.
    pub fn price_history_re_backfill_interval(&self) -> time::Duration {
        self.price_history_re_backfill_interval
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
    pub fn funding_sync_retry_interval(&self) -> time::Duration {
        self.funding_sync_retry_interval
    }

    /// Returns the timeout duration for waiting on sync status updates.
    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    /// Returns the step size for trailing stoploss adjustments.
    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }

    /// Returns whether to clean up all trades when starting the live trading session.
    pub fn startup_clean_up_trades(&self) -> bool {
        self.startup_clean_up_trades
    }

    /// Returns whether to recover existing trades when starting the live trading session.
    pub fn startup_recover_trades(&self) -> bool {
        self.startup_recover_trades
    }

    /// Returns the interval for refreshing and validating the trading session state.
    pub fn trading_session_refresh_interval(&self) -> time::Duration {
        self.trading_session_refresh_interval
    }

    /// Returns whether to clean up all trades when shutting down the live trading session.
    pub fn shutdown_clean_up_trades(&self) -> bool {
        self.shutdown_clean_up_trades
    }

    /// Returns the estimated fee percentage used for trade calculations.
    pub fn trade_estimated_fee(&self) -> PercentageCapped {
        self.trade_estimated_fee
    }

    /// Returns the maximum number of trades that can be running concurrently.
    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    /// Returns the interval for restarting the live process after recoverable errors.
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
    pub fn with_api_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_cooldown = time::Duration::from_secs(secs);
        self
    }

    /// Sets the cooldown period after REST API errors before retrying.
    ///
    /// Default: `10` seconds
    pub fn with_api_error_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    /// Sets the maximum number of retry attempts for REST API errors.
    ///
    /// Default: `3`
    pub fn with_api_error_max_trials(mut self, max_trials: NonZeroU64) -> Self {
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

    /// Sets whether full synchronization mode is enabled (includes complete historical data).
    ///
    /// Default: `false`
    pub fn with_sync_mode_full(mut self, sync_mode_full: bool) -> Self {
        self.sync_mode_full = sync_mode_full;
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

    /// Sets the interval for re-backfilling price history data in backfill mode.
    ///
    /// Default: `90` seconds
    pub fn with_price_history_re_backfill_interval(mut self, secs: u64) -> Self {
        self.price_history_re_backfill_interval = time::Duration::from_secs(secs);
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

    /// Sets the maximum interval between live price ticks before considering the connection
    /// stale.
    ///
    /// Default: `180` seconds (3 minutes)
    pub fn with_live_price_tick_max_interval(mut self, secs: u64) -> Self {
        self.live_price_tick_max_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the retry interval for funding settlement sync when not yet caught up.
    ///
    /// Default: `60` seconds (1 minute)
    pub fn with_funding_sync_retry_interval(mut self, secs: u64) -> Self {
        self.funding_sync_retry_interval = time::Duration::from_secs(secs);
        self
    }

    /// Sets the timeout duration for waiting on sync status updates.
    ///
    /// Default: `5` seconds
    pub fn with_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    /// Sets the step size for trailing stoploss adjustments.
    ///
    /// Default: `PercentageCapped::MIN`
    pub fn with_trailing_stoploss_step_size(
        mut self,
        trade_tsl_step_size: PercentageCapped,
    ) -> Self {
        self.trade_tsl_step_size = trade_tsl_step_size;
        self
    }

    /// Sets whether to clean up all trades when starting the live trading session.
    ///
    /// Default: `false`
    pub fn with_startup_clean_up_trades(mut self, startup_clean_up_trades: bool) -> Self {
        self.startup_clean_up_trades = startup_clean_up_trades;
        self
    }

    /// Sets whether to recover existing trades when starting the live trading session.
    ///
    /// Default: `true`
    pub fn with_startup_recover_trades(mut self, startup_recover_trades: bool) -> Self {
        self.startup_recover_trades = startup_recover_trades;
        self
    }

    /// Sets the interval for refreshing and validating the trading session state.
    ///
    /// Default: `1000` milliseconds (1 second)
    pub fn with_trading_session_refresh_interval(mut self, millis: u64) -> Self {
        self.trading_session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    /// Sets whether to clean up all trades when shutting down the live trading session.
    ///
    /// Default: `false`
    pub fn with_shutdown_clean_up_trades(mut self, shutdown_clean_up_trades: bool) -> Self {
        self.shutdown_clean_up_trades = shutdown_clean_up_trades;
        self
    }

    /// Sets the estimated fee percentage used for trade calculations.
    ///
    /// Default: `0.1%`
    pub fn with_trade_estimated_fee(mut self, trade_estimated_fee: PercentageCapped) -> Self {
        self.trade_estimated_fee = trade_estimated_fee;
        self
    }

    /// Sets the maximum number of trades that can be running concurrently.
    ///
    /// Default: `50`
    pub fn with_trade_max_running_qtd(mut self, trade_max_running_qtd: usize) -> Self {
        self.trade_max_running_qtd = trade_max_running_qtd;
        self
    }

    /// Sets the interval for restarting the live process after recoverable errors.
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

impl From<&LiveTradeConfig> for RestClientConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        RestClientConfig::new(value.rest_api_timeout())
    }
}

impl From<&LiveTradeConfig> for WebSocketClientConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        WebSocketClientConfig::new(value.ws_api_disconnect_timeout())
    }
}

#[derive(Debug)]
pub(super) struct LiveTradeControllerConfig {
    shutdown_timeout: time::Duration,
}

impl LiveTradeControllerConfig {
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }
}

impl From<&LiveTradeConfig> for LiveTradeControllerConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

#[derive(Debug)]
pub(super) struct LiveProcessConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
}

impl LiveProcessConfig {
    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }
}

impl From<&LiveTradeConfig> for LiveProcessConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
        }
    }
}

/// Configuration specific to the live trade executor controlling trade execution parameters and
/// session management.
pub struct LiveTradeExecutorConfig {
    trade_tsl_step_size: PercentageCapped,
    startup_clean_up_trades: bool,
    startup_recover_trades: bool,
    trading_session_refresh_interval: time::Duration,
    shutdown_clean_up_trades: bool,
    trade_estimated_fee: PercentageCapped,
    trade_max_running_qtd: usize,
}

impl LiveTradeExecutorConfig {
    /// Returns the step size for trailing stoploss adjustments.
    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }

    /// Returns whether to clean up all trades when starting the live trading session.
    pub fn startup_clean_up_trades(&self) -> bool {
        self.startup_clean_up_trades
    }

    /// Returns whether to recover existing trades when starting the live trading session.
    pub fn startup_recover_trades(&self) -> bool {
        self.startup_recover_trades
    }

    /// Returns the interval for refreshing and validating the trading session state.
    pub fn trading_session_refresh_interval(&self) -> time::Duration {
        self.trading_session_refresh_interval
    }

    /// Returns whether to clean up all trades when shutting down the live trading session.
    pub fn shutdown_clean_up_trades(&self) -> bool {
        self.shutdown_clean_up_trades
    }

    /// Returns the estimated fee percentage used for trade calculations.
    pub fn trade_estimated_fee(&self) -> PercentageCapped {
        self.trade_estimated_fee
    }

    /// Returns the maximum number of trades that can be running concurrently.
    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    /// Sets the step size for trailing stoploss adjustments.
    ///
    /// Default: `PercentageCapped::MIN`
    pub fn with_trailing_stoploss_step_size(
        mut self,
        trade_tsl_step_size: PercentageCapped,
    ) -> Self {
        self.trade_tsl_step_size = trade_tsl_step_size;
        self
    }

    /// Sets whether to clean up all trades when starting the live trading session.
    ///
    /// Default: `false`
    pub fn with_startup_clean_up_trades(mut self, startup_clean_up_trades: bool) -> Self {
        self.startup_clean_up_trades = startup_clean_up_trades;
        self
    }

    /// Sets whether to recover existing trades when starting the live trading session.
    ///
    /// Default: `true`
    pub fn with_startup_recover_trades(mut self, startup_recover_trades: bool) -> Self {
        self.startup_recover_trades = startup_recover_trades;
        self
    }

    /// Sets the interval for refreshing and validating the trading session state.
    ///
    /// Default: `1000` milliseconds (1 second)
    pub fn with_trading_session_refresh_interval(mut self, millis: u64) -> Self {
        self.trading_session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    /// Sets whether to clean up all trades when shutting down the live trading session.
    ///
    /// Default: `false`
    pub fn with_shutdown_clean_up_trades(mut self, shutdown_clean_up_trades: bool) -> Self {
        self.shutdown_clean_up_trades = shutdown_clean_up_trades;
        self
    }

    /// Sets the estimated fee percentage used for trade calculations.
    ///
    /// Default: `0.1%`
    pub fn with_trade_estimated_fee(mut self, trade_estimated_fee: PercentageCapped) -> Self {
        self.trade_estimated_fee = trade_estimated_fee;
        self
    }

    /// Sets the maximum number of trades that can be running concurrently.
    ///
    /// Default: `50`
    pub fn with_trade_max_running_qtd(mut self, trade_max_running_qtd: usize) -> Self {
        self.trade_max_running_qtd = trade_max_running_qtd;
        self
    }
}

impl Default for LiveTradeExecutorConfig {
    fn default() -> Self {
        Self {
            trade_tsl_step_size: PercentageCapped::MIN,
            startup_clean_up_trades: false,
            startup_recover_trades: true,
            trading_session_refresh_interval: time::Duration::from_millis(1_000),
            shutdown_clean_up_trades: false,
            trade_estimated_fee: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            trade_max_running_qtd: 50,
        }
    }
}

impl From<&LiveTradeConfig> for LiveTradeExecutorConfig {
    fn from(value: &LiveTradeConfig) -> Self {
        Self {
            trade_tsl_step_size: value.trailing_stoploss_step_size(),
            startup_clean_up_trades: value.startup_clean_up_trades(),
            startup_recover_trades: value.startup_recover_trades(),
            trading_session_refresh_interval: value.trading_session_refresh_interval(),
            shutdown_clean_up_trades: value.shutdown_clean_up_trades(),
            trade_estimated_fee: value.trade_estimated_fee(),
            trade_max_running_qtd: value.trade_max_running_qtd(),
        }
    }
}
