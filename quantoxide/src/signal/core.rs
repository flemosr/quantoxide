use std::{fmt, panic::AssertUnwindSafe};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::FutureExt;

use crate::{
    db::models::OhlcCandleRow,
    shared::{LookbackPeriod, MinIterationInterval},
    util::DateTimeExt,
};

use super::{
    error::{SignalValidationError, ValidationResult},
    process::error::{ProcessRecoverableResult, SignalProcessRecoverableError},
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
        candles: &[OhlcCandleRow],
    ) -> std::result::Result<SignalAction, Box<dyn std::error::Error>>;
}

#[async_trait]
impl SignalActionEvaluator for Box<dyn SignalActionEvaluator> {
    async fn evaluate(
        &self,
        candles: &[OhlcCandleRow],
    ) -> std::result::Result<SignalAction, Box<dyn std::error::Error>> {
        (**self).evaluate(candles).await
    }
}

pub struct SignalEvaluator<T: SignalActionEvaluator> {
    name: SignalName,
    min_iteration_interval: MinIterationInterval,
    lookback: Option<LookbackPeriod>,
    action_evaluator: T,
}

impl<T: SignalActionEvaluator> SignalEvaluator<T> {
    pub fn new(
        name: SignalName,
        min_iteration_interval: MinIterationInterval,
        lookback: Option<LookbackPeriod>,
        action_evaluator: T,
    ) -> Self {
        Self {
            name,
            min_iteration_interval,
            lookback,
            action_evaluator,
        }
    }

    pub fn name(&self) -> &SignalName {
        &self.name
    }

    pub fn lookback(&self) -> Option<LookbackPeriod> {
        self.lookback
    }

    pub fn min_iteration_interval(&self) -> MinIterationInterval {
        self.min_iteration_interval
    }

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

pub type ConfiguredSignalEvaluator = SignalEvaluator<Box<dyn SignalActionEvaluator>>;

impl SignalEvaluator<Box<dyn SignalActionEvaluator>> {
    pub fn new_boxed<E>(
        name: SignalName,
        min_iteration_interval: MinIterationInterval,
        lookback: Option<LookbackPeriod>,
        action_evaluator: E,
    ) -> ConfiguredSignalEvaluator
    where
        E: SignalActionEvaluator + 'static,
    {
        Self::new(
            name,
            min_iteration_interval,
            lookback,
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
        candles: &[OhlcCandleRow],
    ) -> ProcessRecoverableResult<Self> {
        let signal_action = evaluator.evaluate(candles).await?;

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
