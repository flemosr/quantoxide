use lnm_sdk::api_v3::models::PercentageCapped;

use crate::shared::Lookback;

use super::error::{BacktestError, Result};

/// Minimum buffer size required for consolidation.
///
/// Derived from [`Lookback::MAX`] expressed in 1-minute candles.
pub(super) const MIN_BUFFER_SIZE: usize = Lookback::MAX.num_minutes() as usize;

/// Configuration for the [`BacktestEngine`](crate::trade::BacktestEngine) controlling simulation
/// parameters and behavior.
pub struct BacktestConfig {
    buffer_size: usize,
    trade_max_running_qtd: usize,
    fee_perc: PercentageCapped,
    trade_tsl_step_size: PercentageCapped,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            buffer_size: MIN_BUFFER_SIZE,
            trade_max_running_qtd: 50,
            fee_perc: 0.1.try_into().expect("must be a valid `PercentageCapped`"),
            trade_tsl_step_size: PercentageCapped::MIN,
        }
    }
}

impl BacktestConfig {
    /// Returns the size of the candlestick buffer used during simulation.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Returns the maximum number of trades that can be running concurrently.
    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    /// Returns the trading fee percentage applied to simulated trades.
    pub fn fee_perc(&self) -> PercentageCapped {
        self.fee_perc
    }

    /// Returns the step size for trailing stoploss adjustments during simulation.
    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }

    /// Sets the size of the candlestick buffer (minimum [`MIN_BUFFER_SIZE`]).
    ///
    /// Default: [`MIN_BUFFER_SIZE`]
    pub fn with_buffer_size(mut self, size: usize) -> Result<Self> {
        if size < MIN_BUFFER_SIZE {
            return Err(BacktestError::InvalidConfigurationBufferSize { size });
        }
        self.buffer_size = size;
        Ok(self)
    }

    /// Sets the maximum number of concurrent running trades (must be greater than 0).
    ///
    /// Default: `50`
    pub fn with_trade_max_running_qtd(mut self, max: usize) -> Result<Self> {
        if max == 0 {
            return Err(BacktestError::InvalidConfigurationMaxRunningQtd { max });
        }
        self.trade_max_running_qtd = max;
        Ok(self)
    }

    /// Sets the trading fee percentage applied to simulated trades.
    ///
    /// Default: `0.1%`
    pub fn with_fee_perc(mut self, fee_perc: PercentageCapped) -> Self {
        self.fee_perc = fee_perc;
        self
    }

    /// Sets the step size for trailing stoploss adjustments during simulation.
    ///
    /// Default: `PercentageCapped::MIN`
    pub fn with_trailing_stoploss_step_size(
        mut self,
        trade_tsl_step_size: PercentageCapped,
    ) -> Self {
        self.trade_tsl_step_size = trade_tsl_step_size;
        self
    }
}

pub(super) struct SimulatedTradeExecutorConfig {
    trade_max_running_qtd: usize,
    fee_perc: PercentageCapped,
    trade_tsl_step_size: PercentageCapped,
}

impl Default for SimulatedTradeExecutorConfig {
    fn default() -> Self {
        Self {
            trade_max_running_qtd: 50,
            fee_perc: 0.1.try_into().expect("must be a valid `PercentageCapped`"),
            trade_tsl_step_size: PercentageCapped::MIN,
        }
    }
}

impl SimulatedTradeExecutorConfig {
    pub fn trade_max_running_qtd(&self) -> usize {
        self.trade_max_running_qtd
    }

    pub fn fee_perc(&self) -> PercentageCapped {
        self.fee_perc
    }

    pub fn trailing_stoploss_step_size(&self) -> PercentageCapped {
        self.trade_tsl_step_size
    }
}

impl From<&BacktestConfig> for SimulatedTradeExecutorConfig {
    fn from(value: &BacktestConfig) -> Self {
        Self {
            trade_max_running_qtd: value.trade_max_running_qtd,
            fee_perc: value.fee_perc,
            trade_tsl_step_size: value.trade_tsl_step_size,
        }
    }
}
