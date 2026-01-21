//! Template implementation of a `SignalEvaluator`.
//!
//! This example demonstrates a reusable evaluator pattern. Evaluators are generic over the target
//! signal type `S`, allowing them to be composed with different operators.

// Remove during implementation
#![allow(unused)]

use std::{fmt, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use quantoxide::{
    error::Result,
    models::{Lookback, MinIterationInterval, OhlcCandleRow, OhlcResolution},
    signal::{Signal, SignalEvaluator},
    tui::TuiLogger,
};

/// Actions that can be signaled by this evaluator.
#[derive(Debug, Clone)]
pub enum SignalAction {
    Long { price: f64, strength: u8 },
    Short { price: f64, strength: u8 },
    CloseLong,
    CloseShort,
    Wait,
}

impl fmt::Display for SignalAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Long { price, strength } => write!(f, "Long @ {price:.2} (strength: {strength})"),
            Self::Short { price, strength } => {
                write!(f, "Short @ {price:.2} (strength: {strength})")
            }
            Self::CloseLong => write!(f, "Close Long"),
            Self::CloseShort => write!(f, "Close Short"),
            Self::Wait => write!(f, "Wait"),
        }
    }
}

/// The native signal type produced by this evaluator.
///
/// Contains the evaluation time and action. Consumers can include any fields they need.
#[derive(Debug, Clone)]
pub struct SignalTemplate {
    pub time: DateTime<Utc>,
    pub action: SignalAction,
    // Add additional fields as needed, e.g.:
    // pub indicator_value: f64,
    // pub confidence: f64,
}

impl fmt::Display for SignalTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}",
            self.time.format("%Y-%m-%d %H:%M:%S"),
            self.action
        )
    }
}

/// A template signal evaluator demonstrating the reusable evaluator pattern.
///
/// The struct itself is not generic, the generic parameter `S` is only on the trait impl.
/// This allows the same evaluator to implement `SignalEvaluator<S>` for any `S` where
/// `SignalTemplate: Into<S>`. When `S` is `SignalTemplate`, no `From` implementation is needed
/// (uses identity conversion). When `S` is a unified enum type, consumers must implement
/// `From<SignalTemplate> for S`.
pub struct SignalEvaluatorTemplate {
    logger: Option<Arc<dyn TuiLogger>>,
}

impl SignalEvaluatorTemplate {
    /// Creates a new evaluator as a boxed trait object.
    ///
    /// The type parameter `S` specifies the target signal type. Use turbofish syntax
    /// to specify it: `SignalEvaluatorTemplate::new::<SupportedSignal>()`.
    pub fn new<S: Signal>() -> Box<dyn SignalEvaluator<S>>
    where
        SignalTemplate: Into<S>,
    {
        Box::new(Self { logger: None })
    }

    /// Creates a new evaluator with TUI logging support as a boxed trait object.
    ///
    /// The type parameter `S` specifies the target signal type. Use turbofish syntax
    /// to specify it: `SignalEvaluatorTemplate::with_logger::<SupportedSignal>(logger)`.
    pub fn with_logger<S: Signal>(logger: Arc<dyn TuiLogger>) -> Box<dyn SignalEvaluator<S>>
    where
        SignalTemplate: Into<S>,
    {
        Box::new(Self {
            logger: Some(logger),
        })
    }

    #[allow(dead_code)]
    async fn log(&self, text: String) -> Result<()> {
        if let Some(logger) = self.logger.as_ref() {
            logger.log(text).await?;
        }
        Ok(())
    }
}

impl Default for SignalEvaluatorTemplate {
    fn default() -> Self {
        Self { logger: None }
    }
}

#[async_trait]
impl<S: Signal> SignalEvaluator<S> for SignalEvaluatorTemplate
where
    SignalTemplate: Into<S>,
{
    fn lookback(&self) -> Option<Lookback> {
        // Use 15-minute candles with a 10-candle period
        Some(Lookback::new(OhlcResolution::FifteenMinutes, 10).expect("valid lookback"))
    }

    fn min_iteration_interval(&self) -> MinIterationInterval {
        // Minimum iteration interval of 5 seconds
        MinIterationInterval::MIN
    }

    async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<S> {
        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;
        //
        // NOTE: `println!` and other `stdout`/`stderr` outputs should be avoided when using TUIs,
        // as they would disrupt rendering.

        let Some(last_candle) = candles.last() else {
            return Err("no candles were provided".into());
        };

        // Evaluate candles and construct the signal
        let signal = SignalTemplate {
            time: last_candle.time,
            action: SignalAction::Wait,
        };

        // Convert to target signal type and return
        Ok(signal.into())
    }
}
