use chrono::{DateTime, Utc};

use crate::{signal::eval::SignalEvaluator, util::DateTimeExt};

pub mod error;

use error::{BacktestError, Result};

const BUFFER_SIZE_DEFAULT: usize = 1800;

pub struct BacktestConfig {
    buffer_size: usize,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            buffer_size: BUFFER_SIZE_DEFAULT,
        }
    }
}

impl BacktestConfig {
    pub fn set_buffer_size(mut self, size: usize) -> Result<Self> {
        if size < 100 {
            return Err(BacktestError::Generic(
                "Buffer size must be at least 100".to_string(),
            ));
        }
        self.buffer_size = size;
        Ok(self)
    }
}

pub struct Backtest {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    evaluators: Vec<Box<dyn SignalEvaluator>>,
}

impl Backtest {
    pub fn new(
        config: BacktestConfig,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        evaluators: Vec<Box<dyn SignalEvaluator>>,
    ) -> Result<Self> {
        if !start_time.is_round() || !end_time.is_round() {
            return Err(BacktestError::Generic(
                "Start and end times must be rounded to seconds".to_string(),
            ));
        }

        // Validate duration is at least 1 day
        if end_time - start_time < chrono::Duration::days(1) {
            return Err(BacktestError::Generic(
                "Backtest duration must be at least 1 day".to_string(),
            ));
        }

        if evaluators.is_empty() {
            return Err(BacktestError::Generic(
                "At least one evaluator must be provided".to_string(),
            ));
        }

        let max_ctx_window = evaluators
            .iter()
            .map(|evaluator| evaluator.context_window_secs())
            .max()
            .expect("evaluators can't be empty");

        if config.buffer_size < max_ctx_window {
            return Err(BacktestError::Generic(format!(
                "buffer size {} is incompatible with max ctx window {}",
                config.buffer_size, max_ctx_window
            )));
        }

        Ok(Self {
            start_time,
            end_time,
            evaluators,
        })
    }
}
