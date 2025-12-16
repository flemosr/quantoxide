//! Template implementation of a `SignalActionEvaluator`.

use std::sync::Arc;

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{LookbackPeriod, MinIterationInterval, OhlcCandleRow},
    signal::{
        ConfiguredSignalEvaluator, SignalAction, SignalActionEvaluator, SignalEvaluator, SignalName,
    },
    tui::TuiLogger,
};

pub struct SignalEvaluatorTemplate {
    logger: Option<Arc<dyn TuiLogger>>,
}

impl SignalEvaluatorTemplate {
    pub fn new(logger: Option<Arc<dyn TuiLogger>>) -> ConfiguredSignalEvaluator {
        let name = SignalName::new("my-sinal-name").expect("name is valid");
        let min_iteration_interval = MinIterationInterval::MIN; // Minimum iteration interval of 5 seconds
        let lookback = LookbackPeriod::MIN; // Last 5 candles (1 min resolution)
        let action_evaluator = Self { logger };

        SignalEvaluator::new_boxed(
            name,
            min_iteration_interval,
            Some(lookback),
            action_evaluator,
        )
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
    async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<SignalAction> {
        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;

        // Evaluate candles return a signal action

        let Some(last_candle) = candles.last() else {
            return Err("no candles were provided".into());
        };

        // Ok(SignalAction::Buy {
        //     price: last_candle.close,
        //     strength: u8::MAX,
        // })

        // Ok(SignalAction::Sell {
        //     price: last_candle.close,
        //     strength: u8::MAX,
        // })

        // Ok(SignalAction::Hold)

        Ok(SignalAction::Wait)
    }
}
