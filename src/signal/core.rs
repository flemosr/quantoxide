use std::{fmt, panic::AssertUnwindSafe};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;

use crate::{
    db::models::OhlcCandleRow,
    error::Result,
    shared::{LookbackPeriod, MinIterationInterval, OhlcResolution},
    util::DateTimeExt,
};

use super::{
    error::{SignalValidationError, ValidationResult},
    process::error::{ProcessRecoverableResult, SignalProcessRecoverableError},
};

/// A validated identifier for a signal evaluator.
///
/// Signal names must be non-empty strings and are used to identify and distinguish different
/// signal evaluators within the system.
#[derive(Debug, Clone)]
pub struct SignalName(String);

impl SignalName {
    /// Creates a new signal name from a string, validating that it is non-empty.
    pub fn new<S>(name: S) -> ValidationResult<Self>
    where
        S: Into<String>,
    {
        let name = name.into();

        if name.is_empty() {
            return Err(SignalValidationError::InvalidSignalNameEmptyString);
        }

        Ok(Self(name))
    }

    /// Returns the signal name as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SignalName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for SignalName {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SignalName {}

/// Represents a trading signal action with associated price and strength information.
///
/// Signal actions are the core output of signal evaluators, indicating what trading decision should
/// be made based on market analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalAction {
    /// Indicates a buy opportunity with the suggested entry price and signal strength (0-100).
    Buy { price: f64, strength: u8 },
    /// Indicates a sell opportunity with the suggested exit price and signal strength (0-100).
    Sell { price: f64, strength: u8 },
    /// Indicates the current position should be maintained without action.
    Hold,
    /// Indicates that no action should be taken.
    Wait,
}

impl fmt::Display for SignalAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buy { price, strength } => {
                write!(f, "Buy(price: {:.1}, strength: {})", price, strength)
            }
            Self::Sell { price, strength } => {
                write!(f, "Sell(price: {:.1}, strength: {})", price, strength)
            }
            Self::Hold => write!(f, "Hold"),
            Self::Wait => write!(f, "Wait"),
        }
    }
}

/// Trait for implementing custom signal evaluation logic. Signal evaluators analyze candlestick
/// data to produce trading signals.
#[async_trait]
pub trait SignalActionEvaluator: Send + Sync {
    /// Evaluates a series of OHLC candlesticks and returns a signal action.
    ///
    /// The candlestick slice is ordered chronologically, with the most recent candle last.
    async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<SignalAction>;
}

#[async_trait]
impl SignalActionEvaluator for Box<dyn SignalActionEvaluator> {
    async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<SignalAction> {
        (**self).evaluate(candles).await
    }
}

/// Complete configuration for a signal evaluator including timing, resolution, and lookback
/// parameters.
///
/// Wraps a [`SignalActionEvaluator`] with metadata controlling when evaluations occur, what candle
/// resolution to use, and how much historical data is provided to the evaluator.
pub struct SignalEvaluator<T: SignalActionEvaluator> {
    name: SignalName,
    resolution: OhlcResolution,
    lookback: Option<LookbackPeriod>,
    min_iteration_interval: MinIterationInterval,
    action_evaluator: T,
}

impl<T: SignalActionEvaluator> SignalEvaluator<T> {
    /// Creates a new signal evaluator with the specified configuration.
    pub fn new(
        name: SignalName,
        resolution: OhlcResolution,
        lookback: Option<LookbackPeriod>,
        min_iteration_interval: MinIterationInterval,
        action_evaluator: T,
    ) -> Self {
        Self {
            name,
            resolution,
            lookback,
            min_iteration_interval,
            action_evaluator,
        }
    }

    /// Returns the name identifier for this signal evaluator.
    pub fn name(&self) -> &SignalName {
        &self.name
    }

    /// Returns the candle resolution for this signal evaluator.
    pub fn resolution(&self) -> OhlcResolution {
        self.resolution
    }

    /// Returns the lookback period determining how many candles of historical data to provide for
    /// evaluation.
    pub fn lookback(&self) -> Option<LookbackPeriod> {
        self.lookback
    }

    /// Returns the minimum interval between successive evaluations.
    pub fn min_iteration_interval(&self) -> MinIterationInterval {
        self.min_iteration_interval
    }

    /// Evaluates candlestick data using the configured action evaluator with panic protection.
    pub async fn evaluate(
        &self,
        candles: &[OhlcCandleRow],
    ) -> ProcessRecoverableResult<SignalAction> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.action_evaluator.evaluate(candles)))
            .await
            .map_err(|e| SignalProcessRecoverableError::EvaluatePanicked(e.into()))?
            .map_err(|e| SignalProcessRecoverableError::EvaluateError(e.to_string()))
    }
}

/// Type alias for a signal evaluator using dynamic dispatch.
///
/// This allows signal evaluators with different concrete types to be stored together in
/// collections.
pub type ConfiguredSignalEvaluator = SignalEvaluator<Box<dyn SignalActionEvaluator>>;

impl SignalEvaluator<Box<dyn SignalActionEvaluator>> {
    /// Creates a new boxed signal evaluator from any implementation of [`SignalActionEvaluator`].
    ///
    /// This constructor enables type erasure, allowing evaluators of different concrete types to be
    /// used interchangeably.
    pub fn new_boxed<E>(
        name: SignalName,
        resolution: OhlcResolution,
        lookback: Option<LookbackPeriod>,
        min_iteration_interval: MinIterationInterval,
        action_evaluator: E,
    ) -> ConfiguredSignalEvaluator
    where
        E: SignalActionEvaluator + 'static,
    {
        Self::new(
            name,
            resolution,
            lookback,
            min_iteration_interval,
            Box::new(action_evaluator),
        )
    }
}

/// A timestamped trading signal produced by a named signal evaluator.
///
/// Signals combine the evaluation result with metadata about when it was generated and which
/// evaluator produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    time: DateTime<Utc>,
    name: SignalName,
    action: SignalAction,
}

impl Signal {
    pub(crate) async fn try_evaluate(
        evaluator: &ConfiguredSignalEvaluator,
        time: DateTime<Utc>,
        candles: &[OhlcCandleRow],
    ) -> ProcessRecoverableResult<Self> {
        let signal_action = evaluator.evaluate(candles).await?;

        Ok(Signal {
            time,
            name: evaluator.name().clone(),
            action: signal_action,
        })
    }

    /// Returns the timestamp when this signal was generated.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Returns the name of the signal evaluator that produced this signal.
    pub fn name(&self) -> &SignalName {
        &self.name
    }

    /// Returns the [`SignalAction`] corresponding to the signal.
    pub fn action(&self) -> SignalAction {
        self.action
    }

    /// Returns a formatted string representation of the signal data for display purposes.
    pub fn as_data_str(&self) -> String {
        format!(
            "time: {}\nname: {}\naction: {}",
            self.time.format_local_secs(),
            self.name,
            self.action
        )
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signal:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
