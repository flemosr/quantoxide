use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
};

use async_trait::async_trait;
use futures::FutureExt;

use crate::{
    db::models::OhlcCandleRow, error::Result, shared::Lookback, shared::MinIterationInterval,
};

use super::process::error::{ProcessRecoverableResult, SignalProcessRecoverableError};

/// Marker trait for signal types that can be used with the signal framework.
///
/// This trait bundles the common constraints required for signal types:
/// - `Send + Sync`: Safe to share across threads
/// - `Clone`: Can be duplicated for broadcasting
/// - `Display`: Can be formatted for logging
/// - `'static`: No borrowed references
///
/// A blanket implementation is provided for all types meeting these constraints, so this trait
/// doesn't need to be implemented manually.
///
/// # Example
///
/// ```
/// # use std::fmt;
/// # use chrono::{DateTime, Utc};
/// #[derive(Debug, Clone)]
/// pub struct MySignal {
///     pub time: DateTime<Utc>,
///     // ...
/// }
///
/// impl fmt::Display for MySignal {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "{}", self.time)
///     }
/// }
///
/// // MySignal: Signal is satisfied automatically
/// ```
pub trait Signal: Send + Sync + Clone + fmt::Display + 'static {}

impl<T> Signal for T where T: Send + Sync + Clone + fmt::Display + 'static {}

/// Trait for implementing custom signal evaluation logic.
///
/// Signal evaluators analyze candlestick data to produce trading signals of type `S`. Evaluators
/// are designed to be reusable building blocks that can be composed into different operators.
///
/// # Type Parameter
///
/// * `S` - The signal type this evaluator produces. For reusable evaluators, this is typically
///   constrained with `where YourSignal: Into<S>` to allow conversion to any target type.
///
/// # Example
///
/// ```
/// # use std::fmt;
/// # use chrono::{DateTime, Utc};
/// use quantoxide::{
///     error::Result,
///     models::{
///         Lookback, MinIterationInterval, OhlcCandleRow, OhlcResolution
///     },
///     signal::{Signal, SignalEvaluator},
/// };
///
/// // Define the evaluator's native signal type
/// #[derive(Debug, Clone)]
/// pub struct MaCrossSignal {
///     pub time: DateTime<Utc>,
///     pub fast_ma: f64,
///     pub slow_ma: f64,
/// }
///
/// impl fmt::Display for MaCrossSignal {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "MaCross at {}: fast={:.2}, slow={:.2}", self.time, self.fast_ma, self.slow_ma)
///     }
/// }
///
/// // Evaluator struct is not generic, only the trait impl is
/// pub struct MaCrossEvaluator {
///     fast_period: usize,
///     slow_period: usize,
/// }
///
/// impl MaCrossEvaluator {
///     pub fn new(fast_period: usize, slow_period: usize) -> Box<Self> {
///         Box::new(Self { fast_period, slow_period })
///     }
/// }
///
/// #[async_trait::async_trait]
/// impl<S: Signal> SignalEvaluator<S> for MaCrossEvaluator
/// where
///     MaCrossSignal: Into<S>,
/// {
///     fn lookback(&self) -> Option<Lookback> {
///         Some(Lookback::new(OhlcResolution::FifteenMinutes, self.slow_period as u64)
///             .expect("valid lookback"))
///     }
///
///     fn min_iteration_interval(&self) -> MinIterationInterval {
///         MinIterationInterval::MIN
///     }
///
///     async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<S> {
///         let signal = MaCrossSignal {
///             time: Utc::now(),
///             fast_ma: 20.0, // Calculate actual MA
///             slow_ma: 100.0,
///         };
///
///         Ok(signal.into()) // Convert to target type
///     }
/// }
/// ```
#[async_trait]
pub trait SignalEvaluator<S: Signal>: Send + Sync {
    /// Returns the candle resolution and count needed for evaluation, or `None` if no historical
    /// candle data is required.
    ///
    /// The framework uses this to fetch the appropriate historical candles before calling
    /// [`evaluate`](Self::evaluate). When `None` is returned, an empty slice is provided to
    /// `evaluate`.
    fn lookback(&self) -> Option<Lookback>;

    /// Returns the minimum interval between successive evaluations.
    ///
    /// The framework will not call [`evaluate`](Self::evaluate) more frequently than this interval.
    fn min_iteration_interval(&self) -> MinIterationInterval;

    /// Evaluates a series of OHLC candlesticks and returns a signal.
    ///
    /// The candlestick slice is ordered chronologically, with the most recent candle last.
    /// The number of candles provided is determined by the [`lookback`](Self::lookback)
    /// configuration.
    async fn evaluate(&self, candles: &[OhlcCandleRow]) -> Result<S>;
}

/// Internal wrapper that provides panic protection for signal evaluators.
pub(crate) struct WrappedSignalEvaluator<S: Signal>(Box<dyn SignalEvaluator<S>>);

impl<S: Signal> WrappedSignalEvaluator<S> {
    pub fn new(evaluator: Box<dyn SignalEvaluator<S>>) -> Self {
        Self(evaluator)
    }

    /// Returns the lookback configuration with panic protection.
    pub fn lookback(&self) -> ProcessRecoverableResult<Option<Lookback>> {
        panic::catch_unwind(AssertUnwindSafe(|| self.0.lookback()))
            .map_err(|e| SignalProcessRecoverableError::LookbackPanicked(e.into()))
    }

    /// Returns the minimum iteration interval with panic protection.
    pub fn min_iteration_interval(&self) -> ProcessRecoverableResult<MinIterationInterval> {
        panic::catch_unwind(AssertUnwindSafe(|| self.0.min_iteration_interval()))
            .map_err(|e| SignalProcessRecoverableError::MinIterationIntervalPanicked(e.into()))
    }

    /// Evaluates candlestick data with panic protection.
    pub async fn evaluate(&self, candles: &[OhlcCandleRow]) -> ProcessRecoverableResult<S> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.evaluate(candles)))
            .await
            .map_err(|e| SignalProcessRecoverableError::EvaluatePanicked(e.into()))?
            .map_err(|e| SignalProcessRecoverableError::EvaluateError(e.to_string()))
    }
}
