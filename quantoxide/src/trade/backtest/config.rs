use chrono::Duration;

use lnm_sdk::api_v2::models::BoundedPercentage;

use super::error::{BacktestError, Result};

pub struct BacktestConfig {
    buffer_size: usize,
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    tsl_step_size: BoundedPercentage,
    update_interval: Duration,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1800,
            max_running_qtd: 50,
            fee_perc: 0.1.try_into().expect("must be a valid `BoundedPercentage`"),
            tsl_step_size: BoundedPercentage::MIN,
            update_interval: Duration::days(1),
        }
    }
}

impl BacktestConfig {
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
    }

    pub fn fee_perc(&self) -> BoundedPercentage {
        self.fee_perc
    }

    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }

    pub fn update_interval(&self) -> Duration {
        self.update_interval
    }

    pub fn with_buffer_size(mut self, size: usize) -> Result<Self> {
        if size < 100 {
            return Err(BacktestError::InvalidConfigurationBufferSize { size });
        }
        self.buffer_size = size;
        Ok(self)
    }

    pub fn with_max_running_qtd(mut self, max: usize) -> Result<Self> {
        if max == 0 {
            return Err(BacktestError::InvalidConfigurationMaxRunningQtd { max });
        }
        self.max_running_qtd = max;
        Ok(self)
    }

    pub fn with_fee_perc(mut self, fee_perc: BoundedPercentage) -> Self {
        self.fee_perc = fee_perc;
        self
    }

    pub fn with_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn with_update_interval(mut self, hours: u32) -> Self {
        self.update_interval = Duration::hours(hours as i64);
        self
    }
}

pub(super) struct SimulatedTradeExecutorConfig {
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    tsl_step_size: BoundedPercentage,
}

impl Default for SimulatedTradeExecutorConfig {
    fn default() -> Self {
        Self {
            max_running_qtd: 50,
            fee_perc: 0.1.try_into().expect("must be a valid `BoundedPercentage`"),
            tsl_step_size: BoundedPercentage::MIN,
        }
    }
}

impl SimulatedTradeExecutorConfig {
    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
    }

    pub fn fee_perc(&self) -> BoundedPercentage {
        self.fee_perc
    }

    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }
}

impl From<&BacktestConfig> for SimulatedTradeExecutorConfig {
    fn from(value: &BacktestConfig) -> Self {
        Self {
            max_running_qtd: value.max_running_qtd,
            fee_perc: value.fee_perc,
            tsl_step_size: value.tsl_step_size,
        }
    }
}
