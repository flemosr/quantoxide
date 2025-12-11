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
    rest_api_timeout: time::Duration,
    ws_api_disconnect_timeout: time::Duration,
    rest_api_cooldown: time::Duration,
    rest_api_error_cooldown: time::Duration,
    rest_api_error_max_trials: NonZeroU64,
    price_history_batch_size: NonZeroU64,
    sync_mode_full: bool,
    price_history_reach: Duration,
    price_history_re_sync_interval: time::Duration,
    live_price_tick_max_interval: time::Duration,
    sync_update_timeout: time::Duration,
    trade_tsl_step_size: PercentageCapped,
    startup_clean_up_trades: bool,
    startup_recover_trades: bool,
    trading_session_ttl: TradingSessionTTL,
    trading_session_refresh_interval: time::Duration,
    shutdown_clean_up_trades: bool,
    trade_estimated_fee: PercentageCapped,
    trade_max_running_qtd: usize,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        let trading_session_ttl = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `TradingSessionTTL`");

        Self {
            rest_api_timeout: time::Duration::from_secs(20),
            ws_api_disconnect_timeout: time::Duration::from_secs(6),
            rest_api_cooldown: time::Duration::from_secs(2),
            rest_api_error_cooldown: time::Duration::from_secs(10),
            rest_api_error_max_trials: 3.try_into().expect("not zero"),
            price_history_batch_size: 1000.try_into().expect("not zero"),
            sync_mode_full: false,
            price_history_reach: Duration::days(90),
            price_history_re_sync_interval: time::Duration::from_secs(10),
            live_price_tick_max_interval: time::Duration::from_mins(3),
            sync_update_timeout: time::Duration::from_secs(5),
            trade_tsl_step_size: PercentageCapped::MIN,
            startup_clean_up_trades: true,
            startup_recover_trades: false,
            trading_session_ttl,
            trading_session_refresh_interval: time::Duration::from_millis(1_000),
            shutdown_clean_up_trades: true,
            trade_estimated_fee: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            trade_max_running_qtd: 50,
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveConfig {
    pub fn rest_api_timeout(&self) -> time::Duration {
        self.rest_api_timeout
    }

    pub fn ws_api_disconnect_timeout(&self) -> time::Duration {
        self.ws_api_disconnect_timeout
    }

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

    pub fn sync_mode_full(&self) -> bool {
        self.sync_mode_full
    }

    pub fn price_history_reach(&self) -> Duration {
        self.price_history_reach
    }

    pub fn price_history_re_sync_interval(&self) -> time::Duration {
        self.price_history_re_sync_interval
    }

    pub fn live_price_tick_max_interval(&self) -> time::Duration {
        self.live_price_tick_max_interval
    }

    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }

    pub fn startup_clean_up_trades(&self) -> bool {
        self.startup_clean_up_trades
    }

    pub fn startup_recover_trades(&self) -> bool {
        self.startup_recover_trades
    }

    pub fn trading_session_ttl(&self) -> TradingSessionTTL {
        self.trading_session_ttl
    }

    pub fn trading_session_refresh_interval(&self) -> time::Duration {
        self.trading_session_refresh_interval
    }

    pub fn shutdown_clean_up_trades(&self) -> bool {
        self.shutdown_clean_up_trades
    }

    pub fn trade_estimated_fee(&self) -> PercentageCapped {
        self.trade_estimated_fee
    }

    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn with_rest_api_timeout(mut self, secs: u64) -> Self {
        self.rest_api_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_ws_api_disconnect_timeout(mut self, secs: u64) -> Self {
        self.ws_api_disconnect_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_error_cooldown(mut self, secs: u64) -> Self {
        self.rest_api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn with_api_error_max_trials(mut self, max_trials: NonZeroU64) -> Self {
        self.rest_api_error_max_trials = max_trials;
        self
    }

    pub fn with_price_history_batch_size(mut self, size: NonZeroU64) -> Self {
        self.price_history_batch_size = size;
        self
    }

    pub fn with_sync_mode_full(mut self, sync_mode_full: bool) -> Self {
        self.sync_mode_full = sync_mode_full;
        self
    }

    pub fn with_price_history_reach(mut self, days: NonZeroU64) -> Self {
        self.price_history_reach = Duration::days(days.get() as i64);
        self
    }

    pub fn with_price_history_re_sync_interval(mut self, secs: u64) -> Self {
        self.price_history_re_sync_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn with_live_price_tick_max_interval(mut self, secs: u64) -> Self {
        self.live_price_tick_max_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn with_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn with_trailing_stoploss_step_size(
        mut self,
        trade_tsl_step_size: PercentageCapped,
    ) -> Self {
        self.trade_tsl_step_size = trade_tsl_step_size;
        self
    }

    pub fn with_startup_clean_up_trades(mut self, startup_clean_up_trades: bool) -> Self {
        self.startup_clean_up_trades = startup_clean_up_trades;
        self
    }

    pub fn with_startup_recover_trades(mut self, startup_recover_trades: bool) -> Self {
        self.startup_recover_trades = startup_recover_trades;
        self
    }

    pub fn with_trading_session_ttl(mut self, trading_session_ttl: TradingSessionTTL) -> Self {
        self.trading_session_ttl = trading_session_ttl;
        self
    }

    pub fn with_trading_session_refresh_interval(mut self, millis: u64) -> Self {
        self.trading_session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    pub fn with_shutdown_clean_up_trades(mut self, shutdown_clean_up_trades: bool) -> Self {
        self.shutdown_clean_up_trades = shutdown_clean_up_trades;
        self
    }

    pub fn with_trade_estimated_fee(mut self, trade_estimated_fee: PercentageCapped) -> Self {
        self.trade_estimated_fee = trade_estimated_fee;
        self
    }

    pub fn with_trade_max_running_qtd(mut self, trade_max_running_qtd: usize) -> Self {
        self.trade_max_running_qtd = trade_max_running_qtd;
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
        RestClientConfig::new(value.rest_api_timeout())
    }
}

impl From<&LiveConfig> for WebSocketClientConfig {
    fn from(value: &LiveConfig) -> Self {
        WebSocketClientConfig::new(value.ws_api_disconnect_timeout())
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
    trade_tsl_step_size: PercentageCapped,
    startup_clean_up_trades: bool,
    startup_recover_trades: bool,
    trading_session_ttl: TradingSessionTTL,
    trading_session_refresh_interval: time::Duration,
    shutdown_clean_up_trades: bool,
    trade_estimated_fee: PercentageCapped,
    trade_max_running_qtd: usize,
}

impl LiveTradeExecutorConfig {
    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }

    pub fn startup_clean_up_trades(&self) -> bool {
        self.startup_clean_up_trades
    }

    pub fn startup_recover_trades(&self) -> bool {
        self.startup_recover_trades
    }

    pub fn trading_session_ttl(&self) -> TradingSessionTTL {
        self.trading_session_ttl
    }

    pub fn trading_session_refresh_interval(&self) -> time::Duration {
        self.trading_session_refresh_interval
    }

    pub fn shutdown_clean_up_trades(&self) -> bool {
        self.shutdown_clean_up_trades
    }

    pub fn trade_estimated_fee(&self) -> PercentageCapped {
        self.trade_estimated_fee
    }

    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    pub fn with_trailing_stoploss_step_size(
        mut self,
        trade_tsl_step_size: PercentageCapped,
    ) -> Self {
        self.trade_tsl_step_size = trade_tsl_step_size;
        self
    }

    pub fn with_startup_clean_up_trades(mut self, startup_clean_up_trades: bool) -> Self {
        self.startup_clean_up_trades = startup_clean_up_trades;
        self
    }

    pub fn with_startup_recover_trades(mut self, startup_recover_trades: bool) -> Self {
        self.startup_recover_trades = startup_recover_trades;
        self
    }

    pub fn with_trading_session_ttl(mut self, trading_session_ttl: TradingSessionTTL) -> Self {
        self.trading_session_ttl = trading_session_ttl;
        self
    }

    pub fn with_trading_session_refresh_interval(mut self, millis: u64) -> Self {
        self.trading_session_refresh_interval = time::Duration::from_millis(millis);
        self
    }

    pub fn with_shutdown_clean_up_trades(mut self, shutdown_clean_up_trades: bool) -> Self {
        self.shutdown_clean_up_trades = shutdown_clean_up_trades;
        self
    }

    pub fn with_trade_estimated_fee(mut self, trade_estimated_fee: PercentageCapped) -> Self {
        self.trade_estimated_fee = trade_estimated_fee;
        self
    }

    pub fn with_trade_max_running_qtd(mut self, trade_max_running_qtd: usize) -> Self {
        self.trade_max_running_qtd = trade_max_running_qtd;
        self
    }
}

impl Default for LiveTradeExecutorConfig {
    fn default() -> Self {
        let trading_session_ttl = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `TradingSessionTTL`");

        Self {
            trade_tsl_step_size: PercentageCapped::MIN,
            startup_clean_up_trades: true,
            startup_recover_trades: false,
            trading_session_ttl,
            trading_session_refresh_interval: time::Duration::from_millis(1_000),
            shutdown_clean_up_trades: true,
            trade_estimated_fee: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
            trade_max_running_qtd: 50,
        }
    }
}

impl From<&LiveConfig> for LiveTradeExecutorConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            trade_tsl_step_size: value.trailing_stoploss_step_size(),
            startup_clean_up_trades: value.startup_clean_up_trades(),
            startup_recover_trades: value.startup_recover_trades(),
            trading_session_ttl: value.trading_session_ttl(),
            trading_session_refresh_interval: value.trading_session_refresh_interval(),
            shutdown_clean_up_trades: value.shutdown_clean_up_trades(),
            trade_estimated_fee: value.trade_estimated_fee(),
            trade_max_running_qtd: value.trade_max_running_qtd(),
        }
    }
}
