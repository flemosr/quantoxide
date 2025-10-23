use std::{fmt, num::NonZeroU64, panic::AssertUnwindSafe};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::FutureExt;

use crate::{db::models::PriceHistoryEntryLOCF, util::DateTimeExt};

use super::{
    error::{SignalValidationError, ValidationResult},
    process::error::{ProcessResult, SignalProcessError},
};

#[derive(Debug, Clone)]
pub struct SignalName(String);

impl SignalName {
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalAction {
    Buy { price: f64, strength: u8 },
    Sell { price: f64, strength: u8 },
    Hold,
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

#[async_trait]
pub trait SignalActionEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        entries: &[PriceHistoryEntryLOCF],
    ) -> std::result::Result<SignalAction, Box<dyn std::error::Error>>;
}

#[async_trait]
impl SignalActionEvaluator for Box<dyn SignalActionEvaluator> {
    async fn evaluate(
        &self,
        entries: &[PriceHistoryEntryLOCF],
    ) -> std::result::Result<SignalAction, Box<dyn std::error::Error>> {
        (**self).evaluate(entries).await
    }
}

pub struct SignalEvaluator<T: SignalActionEvaluator> {
    name: SignalName,
    evaluation_interval: Duration,
    context_window_secs: usize,
    action_evaluator: T,
}

impl<T: SignalActionEvaluator> SignalEvaluator<T> {
    pub fn new(
        name: SignalName,
        evaluation_interval_secs: impl TryInto<NonZeroU64>,
        context_window_secs: usize,
        action_evaluator: T,
    ) -> ValidationResult<Self> {
        let evaluation_interval_secs: NonZeroU64 = evaluation_interval_secs
            .try_into()
            .map_err(|_| SignalValidationError::InvalidEvaluationInterval)?;

        Ok(Self {
            name,
            evaluation_interval: Duration::seconds(evaluation_interval_secs.get() as i64),
            context_window_secs,
            action_evaluator,
        })
    }

    pub fn name(&self) -> &SignalName {
        &self.name
    }

    pub fn evaluation_interval(&self) -> Duration {
        self.evaluation_interval
    }

    pub fn context_window_secs(&self) -> usize {
        self.context_window_secs
    }

    pub async fn evaluate(&self, entries: &[PriceHistoryEntryLOCF]) -> ProcessResult<SignalAction> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.action_evaluator.evaluate(entries)))
            .await
            .map_err(|e| SignalProcessError::EvaluatePanicked(e.into()))?
            .map_err(|e| SignalProcessError::EvaluateError(e.to_string()))
    }
}

pub type ConfiguredSignalEvaluator = SignalEvaluator<Box<dyn SignalActionEvaluator>>;

impl SignalEvaluator<Box<dyn SignalActionEvaluator>> {
    pub fn new_boxed<E>(
        name: SignalName,
        evaluation_interval_secs: impl TryInto<NonZeroU64>,
        context_window_secs: usize,
        action_evaluator: E,
    ) -> ValidationResult<ConfiguredSignalEvaluator>
    where
        E: SignalActionEvaluator + 'static,
    {
        Self::new(
            name,
            evaluation_interval_secs,
            context_window_secs,
            Box::new(action_evaluator),
        )
    }
}

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
        entries: &[PriceHistoryEntryLOCF],
    ) -> ProcessResult<Self> {
        let signal_action = evaluator.evaluate(entries).await?;

        Ok(Signal {
            time,
            name: evaluator.name().clone(),
            action: signal_action,
        })
    }

    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn name(&self) -> &SignalName {
        &self.name
    }

    pub fn action(&self) -> SignalAction {
        self.action
    }

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
