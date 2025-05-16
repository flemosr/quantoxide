use std::{fmt, panic::AssertUnwindSafe};

use async_trait::async_trait;
use futures::FutureExt;

use crate::db::models::PriceHistoryEntryLOCF;

use super::error::{Result, SignalError};

#[derive(Debug, Clone)]
pub struct SignalName(String);

impl SignalName {
    pub fn new<S>(name: S) -> Result<Self>
    where
        S: Into<String>,
    {
        let name = name.into();

        if name.is_empty() {
            return Err(SignalError::Generic(
                "signal name cannot be empty".to_string(),
            ));
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
    context_window_secs: usize,
    action_evaluator: T,
}

impl<T: SignalActionEvaluator> SignalEvaluator<T> {
    pub fn new(name: SignalName, context_window_secs: usize, action_evaluator: T) -> Self {
        Self {
            name,
            context_window_secs,
            action_evaluator,
        }
    }

    pub fn name(&self) -> &SignalName {
        &self.name
    }

    pub fn context_window_secs(&self) -> usize {
        self.context_window_secs
    }

    pub async fn evaluate(&self, entries: &[PriceHistoryEntryLOCF]) -> Result<SignalAction> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.action_evaluator.evaluate(entries)))
            .await
            .map_err(|_| SignalError::Generic(format!("`SignalEvaluator::evaluate` panicked")))?
            .map_err(|e| {
                SignalError::Generic(format!(
                    "`SignalEvaluator::evaluate` error {}",
                    e.to_string()
                ))
            })
    }
}

pub type ConfiguredSignalEvaluator = SignalEvaluator<Box<dyn SignalActionEvaluator>>;

impl SignalEvaluator<Box<dyn SignalActionEvaluator>> {
    pub fn new_boxed<E>(
        name: SignalName,
        context_window_secs: usize,
        action_evaluator: E,
    ) -> ConfiguredSignalEvaluator
    where
        E: SignalActionEvaluator + 'static,
    {
        Self {
            name,
            context_window_secs,
            action_evaluator: Box::new(action_evaluator),
        }
    }
}
