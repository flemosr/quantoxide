//! Template implementation of a `SignalActionEvaluator`.

// Remove during implementation
#![allow(unused)]

use std::sync::Arc;

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{Lookback, MinIterationInterval, OhlcCandleRow, OhlcResolution},
    signal::{
        ConfiguredSignalEvaluator, SignalAction, SignalActionEvaluator, SignalEvaluator,
        SignalExtra, SignalName,
    },
    tui::TuiLogger,
};

pub struct SignalEvaluatorTemplate {
    logger: Option<Arc<dyn TuiLogger>>,
}

impl SignalEvaluatorTemplate {
    pub fn new() -> Self {
        Self { logger: None }
    }

    pub fn with_logger(logger: Arc<dyn TuiLogger>) -> Self {
        Self {
            logger: Some(logger),
        }
    }

    pub fn configure(self) -> ConfiguredSignalEvaluator {
        let name = SignalName::new("my-sinal-name").expect("name is valid");
        // Use 15-minute candles with a 10-candle period
        let lookback = Some(Lookback::new(OhlcResolution::FifteenMinutes, 10).expect("is valid"));
        let min_iteration_interval = MinIterationInterval::MIN; // Minimum iteration interval of 5 seconds

        SignalEvaluator::new_boxed(name, lookback, min_iteration_interval, self)
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
impl SignalActionEvaluator for SignalEvaluatorTemplate {
    #[allow(unused_variables)]
    async fn evaluate(
        &self,
        candles: &[OhlcCandleRow],
    ) -> Result<(SignalAction, Option<SignalExtra>)> {
        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;
        //
        // NOTE: `println!` and other `stdout`/`stderr` outputs should be avoided when using TUIs,
        // as they would disrupt rendering.

        // Evaluate candles and return a signal action with optional extra data

        let Some(last_candle) = candles.last() else {
            return Err("no candles were provided".into());
        };

        // Example: Return action without extra data
        // Ok((SignalAction::Buy {
        //     price: last_candle.close,
        //     strength: u8::MAX,
        // }, None))

        // Example: Return action with extra data (e.g., calculated ATR for dynamic SL/TP)
        // let mut extra = SignalExtra::new();
        // extra.insert("atr".into(), "500.0".into());
        // extra.insert("sl_pct".into(), "2.5".into());
        // extra.insert("tp_pct".into(), "5.0".into());
        //
        // Ok((SignalAction::Sell {
        //     price: last_candle.close,
        //     strength: u8::MAX,
        // }, Some(extra)))

        // Ok((SignalAction::Hold, None))

        Ok((SignalAction::Wait, None))
    }
}
