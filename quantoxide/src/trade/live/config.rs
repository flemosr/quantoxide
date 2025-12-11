use std::num::NonZeroU64;

use chrono::Duration;
use tokio::time;

use lnm_sdk::{
    api_v2::WebSocketClientConfig,
    api_v3::{RestClientConfig, models::PercentageCapped},
};

use super::executor::state::live_trading_session::TradingSessionTTL;

#[derive(Clone, Debug)]
pub struct LiveConfig {
    api_rest_timeout: time::Duration,
    api_ws_disconnect_timeout: time::Duration,
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: NonZeroU64,
    api_history_batch_size: NonZeroU64,
    sync_mode_full: bool,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    max_tick_interval: time::Duration,
    sync_update_timeout: time::Duration,
    tsl_step_size: PercentageCapped,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    session_ttl: TradingSessionTTL,
    session_refresh_interval: time::Duration,
    clean_up_trades_on_shutdown: bool,
    estimated_fee_perc: PercentageCapped,
    max_running_qtd: usize,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        let session_ttl = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `TradingSessionTTL`");

        Self {
            api_rest_timeout: time::Duration::from_secs(20),
            api_ws_disconnect_timeout: time::Duration::from_secs(6),
            api_cooldown: time::Duration::from_secs(2),
            api_error_cooldown: time::Duration::from_secs(10),
            api_error_max_trials: 3.try_into().expect("not zero"),
            api_history_batch_size: 1000.try_into().expect("not zero"),
            sync_mode_full: false,
            sync_history_reach: Duration::days(90),
            re_sync_history_interval: time::Duration::from_secs(10),
            max_tick_interval: time::Duration::from_mins(3),
            sync_update_timeout: time::Duration::from_secs(5),
            tsl_step_size: PercentageCapped::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            session_ttl,
            session_refresh_interval: time::Duration::from_millis(1_000),
            clean_up_trades_on_shutdown: true,
            estimated_fee_perc: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            max_running_qtd: 50,
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveConfig {
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

    pub fn sync_mode_full(&self) -> bool {
        self.sync_mode_full
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

    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn session_ttl(&self) -> TradingSessionTTL {
        self.session_ttl
    }

    pub fn session_refresh_interval(&self) -> time::Duration {
        self.session_refresh_interval
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn estimated_fee_perc(&self) -> PercentageCapped {
        self.estimated_fee_perc
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
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

    pub fn with_sync_mode_full(mut self, sync_mode_full: bool) -> Self {
        self.sync_mode_full = sync_mode_full;
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

    pub fn with_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_trailing_stoploss_step_size(mut self, tsl_step_size: PercentageCapped) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn with_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn with_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn with_session_ttl(mut self, session_ttl: TradingSessionTTL) -> Self {
        self.session_ttl = session_ttl;
        self
    }

    pub fn with_session_refresh_interval(mut self, millis: u64) -> Self {
        self.session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    pub fn with_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }

    pub fn with_estimated_fee_perc(mut self, estimated_fee_perc: PercentageCapped) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn with_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
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

impl From<&LiveConfig> for RestClientConfig {
    fn from(value: &LiveConfig) -> Self {
        RestClientConfig::new(value.api_rest_timeout())
    }
}

impl From<&LiveConfig> for WebSocketClientConfig {
    fn from(value: &LiveConfig) -> Self {
        WebSocketClientConfig::new(value.api_ws_disconnect_timeout())
    }
}

#[derive(Debug)]
pub(super) struct LiveControllerConfig {
    shutdown_timeout: time::Duration,
}

impl LiveControllerConfig {
    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }
}

impl From<&LiveConfig> for LiveControllerConfig {
    fn from(value: &LiveConfig) -> Self {
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

impl From<&LiveConfig> for LiveProcessConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
        }
    }
}

pub struct LiveTradeExecutorConfig {
    api_rest_timeout: time::Duration,
    api_ws_disconnect_timeout: time::Duration,
    tsl_step_size: PercentageCapped,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    session_ttl: TradingSessionTTL,
    session_refresh_interval: time::Duration,
    clean_up_trades_on_shutdown: bool,
    estimated_fee_perc: PercentageCapped,
    max_running_qtd: usize,
    max_tick_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl LiveTradeExecutorConfig {
    pub fn api_rest_timeout(&self) -> time::Duration {
        self.api_rest_timeout
    }

    pub fn api_ws_disconnect_timeout(&self) -> time::Duration {
        self.api_ws_disconnect_timeout
    }

    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn session_ttl(&self) -> TradingSessionTTL {
        self.session_ttl
    }

    pub fn session_refresh_interval(&self) -> time::Duration {
        self.session_refresh_interval
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn estimated_fee_perc(&self) -> PercentageCapped {
        self.estimated_fee_perc
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
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

    pub fn with_trailing_stoploss_step_size(mut self, tsl_step_size: PercentageCapped) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn with_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn with_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn with_session_ttl(mut self, session_ttl: TradingSessionTTL) -> Self {
        self.session_ttl = session_ttl;
        self
    }

    pub fn with_session_refresh_interval(mut self, millis: u64) -> Self {
        self.session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    pub fn with_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }

    pub fn with_estimated_fee_perc(mut self, estimated_fee_perc: PercentageCapped) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn with_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
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

impl Default for LiveTradeExecutorConfig {
    fn default() -> Self {
        let session_ttl = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `TradingSessionTTL`");

        Self {
            api_rest_timeout: time::Duration::from_secs(20),
            api_ws_disconnect_timeout: time::Duration::from_secs(6),
            tsl_step_size: PercentageCapped::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            session_ttl,
            session_refresh_interval: time::Duration::from_millis(1_000),
            clean_up_trades_on_shutdown: true,
            estimated_fee_perc: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            max_running_qtd: 50,
            max_tick_interval: time::Duration::from_mins(3),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl From<&LiveTradeExecutorConfig> for RestClientConfig {
    fn from(value: &LiveTradeExecutorConfig) -> Self {
        RestClientConfig::new(value.api_rest_timeout())
    }
}

impl From<&LiveTradeExecutorConfig> for WebSocketClientConfig {
    fn from(value: &LiveTradeExecutorConfig) -> Self {
        WebSocketClientConfig::new(value.api_ws_disconnect_timeout())
    }
}

impl From<&LiveConfig> for LiveTradeExecutorConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            api_rest_timeout: value.api_rest_timeout(),
            api_ws_disconnect_timeout: value.api_ws_disconnect_timeout(),
            tsl_step_size: value.trailing_stoploss_step_size(),
            clean_up_trades_on_startup: value.clean_up_trades_on_startup(),
            recover_trades_on_startup: value.recover_trades_on_startup(),
            session_ttl: value.session_ttl(),
            session_refresh_interval: value.session_refresh_interval(),
            clean_up_trades_on_shutdown: value.clean_up_trades_on_shutdown(),
            estimated_fee_perc: value.estimated_fee_perc(),
            max_running_qtd: value.max_running_qtd(),
            max_tick_interval: value.max_tick_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}
