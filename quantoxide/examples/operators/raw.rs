//! Template implementation of a `RawOperator`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use quantoxide::{
    error::Result,
    models::{LookbackPeriod, MinIterationInterval, OhlcCandleRow},
    trade::{RawOperator, TradeExecutor},
    tui::TuiLogger,
};

pub struct RawOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    logger: Option<Arc<dyn TuiLogger>>,
}

impl RawOperatorTemplate {
    pub fn new(logger: Option<Arc<dyn TuiLogger>>) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            logger,
        })
    }

    fn trade_executor(&self) -> Result<&Arc<dyn TradeExecutor>> {
        if let Some(trade_executor) = self.trade_executor.get() {
            return Ok(trade_executor);
        }
        Err("trade executor was not set".into())
    }

    #[allow(dead_code)]
    async fn log(&self, text: String) -> Result<()> {
        if let Some(logger) = self.logger.as_ref() {
            logger.log(text).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl RawOperator for RawOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if let Err(_) = self.trade_executor.set(trade_executor) {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    fn lookback(&self) -> Option<LookbackPeriod> {
        // None // Return no candles
        // Some(LookbackPeriod::try_from(10).expect("is valid")) // Return the last 10 candles
        Some(LookbackPeriod::MIN) // Return the last 5 candles
    }

    fn min_iteration_interval(&self) -> MinIterationInterval {
        // MinIterationInterval::seconds(10).expect("is valid") // Minimum iteration interval of 10 seconds
        MinIterationInterval::MIN // Minimum iteration interval of 5 seconds
    }

    async fn iterate(&self, _candles: &[OhlcCandleRow]) -> Result<()> {
        let _trade_executor = self.trade_executor()?;

        // Evaluate candles and perform trading operations via trade executor
        // ...

        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;

        Ok(())
    }
}
