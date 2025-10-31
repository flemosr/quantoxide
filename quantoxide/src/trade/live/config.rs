use chrono::Duration;
use tokio::time;

use lnm_sdk::api::rest::models::BoundedPercentage;

use super::executor::state::TradingSessionRefreshOffset;

#[derive(Clone, Debug)]
pub struct LiveConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_mode_full: bool,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    sync_update_timeout: time::Duration,
    tsl_step_size: BoundedPercentage,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    session_refresh_offset: TradingSessionRefreshOffset,
    clean_up_trades_on_shutdown: bool,
    estimated_fee_perc: BoundedPercentage,
    max_running_qtd: usize,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        let session_refresh_offset = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `LiveTradingSessionRefreshOffset`");

        Self {
            api_cooldown: time::Duration::from_secs(2),
            api_error_cooldown: time::Duration::from_secs(10),
            api_error_max_trials: 3,
            api_history_batch_size: 1000,
            sync_mode_full: false,
            sync_history_reach: Duration::hours(24 * 7 * 4),
            re_sync_history_interval: time::Duration::from_secs(300),
            sync_update_timeout: time::Duration::from_secs(5),
            tsl_step_size: BoundedPercentage::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            session_refresh_offset,
            clean_up_trades_on_shutdown: true,
            estimated_fee_perc: BoundedPercentage::try_from(0.1)
                .expect("must be valid `BoundedPercentage`"),
            max_running_qtd: 50,
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveConfig {
    pub fn api_cooldown(&self) -> time::Duration {
        self.api_cooldown
    }

    pub fn api_error_cooldown(&self) -> time::Duration {
        self.api_error_cooldown
    }

    pub fn api_error_max_trials(&self) -> u32 {
        self.api_error_max_trials
    }

    pub fn api_history_batch_size(&self) -> usize {
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

    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn session_refresh_offset(&self) -> TradingSessionRefreshOffset {
        self.session_refresh_offset
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn estimated_fee_perc(&self) -> BoundedPercentage {
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

    pub fn set_api_cooldown(mut self, secs: u64) -> Self {
        self.api_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_cooldown(mut self, secs: u64) -> Self {
        self.api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_max_trials(mut self, max_trials: u32) -> Self {
        self.api_error_max_trials = max_trials;
        self
    }

    pub fn set_api_history_batch_size(mut self, size: usize) -> Self {
        self.api_history_batch_size = size;
        self
    }

    pub fn set_sync_mode_full(mut self, sync_mode_full: bool) -> Self {
        self.sync_mode_full = sync_mode_full;
        self
    }

    pub fn set_sync_history_reach(mut self, hours: u64) -> Self {
        self.sync_history_reach = Duration::hours(hours as i64);
        self
    }

    pub fn set_re_sync_history_interval(mut self, secs: u64) -> Self {
        self.re_sync_history_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
        self
    }

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn set_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn set_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn set_session_refresh_offset(
        mut self,
        session_refresh_offset: TradingSessionRefreshOffset,
    ) -> Self {
        self.session_refresh_offset = session_refresh_offset;
        self
    }

    pub fn set_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }

    pub fn set_estimated_fee_perc(mut self, estimated_fee_perc: BoundedPercentage) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn set_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

#[derive(Debug)]
pub struct LiveControllerConfig {
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
pub struct LiveProcessConfig {
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
    tsl_step_size: BoundedPercentage,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    session_refresh_offset: TradingSessionRefreshOffset,
    clean_up_trades_on_shutdown: bool,
    estimated_fee_perc: BoundedPercentage,
    max_running_qtd: usize,
}

impl LiveTradeExecutorConfig {
    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn session_refresh_offset(&self) -> TradingSessionRefreshOffset {
        self.session_refresh_offset
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn estimated_fee_perc(&self) -> BoundedPercentage {
        self.estimated_fee_perc
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
    }

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn set_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn set_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn set_session_refresh_offset(
        mut self,
        session_refresh_offset: TradingSessionRefreshOffset,
    ) -> Self {
        self.session_refresh_offset = session_refresh_offset;
        self
    }

    pub fn set_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }

    pub fn set_estimated_fee_perc(mut self, estimated_fee_perc: BoundedPercentage) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn set_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
        self
    }
}

impl Default for LiveTradeExecutorConfig {
    fn default() -> Self {
        let session_refresh_offset = (Duration::hours(1) + Duration::minutes(5))
            .try_into()
            .expect("must be valid `LiveTradingSessionRefreshOffset`");

        Self {
            tsl_step_size: BoundedPercentage::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            session_refresh_offset,
            clean_up_trades_on_shutdown: true,
            estimated_fee_perc: BoundedPercentage::try_from(0.1)
                .expect("must be valid `BoundedPercentage`"),
            max_running_qtd: 50,
        }
    }
}

impl From<&LiveConfig> for LiveTradeExecutorConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            tsl_step_size: value.trailing_stoploss_step_size(),
            clean_up_trades_on_startup: value.clean_up_trades_on_startup(),
            recover_trades_on_startup: value.recover_trades_on_startup(),
            session_refresh_offset: value.session_refresh_offset(),
            clean_up_trades_on_shutdown: value.clean_up_trades_on_shutdown(),
            estimated_fee_perc: value.estimated_fee_perc(),
            max_running_qtd: value.max_running_qtd(),
        }
    }
}
