use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
};

use async_trait::async_trait;
use futures::FutureExt;

use crate::db::models::PriceHistoryEntryLOCF;

use super::error::{Result, SignalError};

#[derive(Debug, Clone)]
pub struct SignalName(String);

impl SignalName {
    pub fn new(name: String) -> Result<Self> {
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
pub trait SignalEvaluator: Send + Sync {
    fn name(&self) -> &SignalName;

    fn context_window_secs(&self) -> usize;

    async fn evaluate(
        &self,
        entries: &[PriceHistoryEntryLOCF],
    ) -> std::result::Result<SignalAction, Box<dyn std::error::Error>>;
}

pub(crate) struct WrappedSignalEvaluator(Box<dyn SignalEvaluator>);

impl WrappedSignalEvaluator {
    pub fn name(&self) -> Result<&SignalName> {
        panic::catch_unwind(AssertUnwindSafe(|| self.0.name()))
            .map_err(|_| SignalError::Generic(format!("`SignalEvaluator::name` panicked")))
    }

    pub fn context_window_secs(&self) -> Result<usize> {
        panic::catch_unwind(AssertUnwindSafe(|| self.0.context_window_secs())).map_err(|_| {
            SignalError::Generic(format!("`SignalEvaluator::context_window_secs` panicked"))
        })
    }

    pub async fn evaluate(&self, entries: &[PriceHistoryEntryLOCF]) -> Result<SignalAction> {
        FutureExt::catch_unwind(AssertUnwindSafe(self.0.evaluate(entries)))
            .await
            .map_err(|_| SignalError::Generic(format!("`SignalEvaluator::evaluate` panicked")))?
            .map_err(|e| {
                SignalError::Generic(format!(
                    "`SignalEvaluator::evaluate`   error {}",
                    e.to_string()
                ))
            })
    }
}

impl From<Box<dyn SignalEvaluator>> for WrappedSignalEvaluator {
    fn from(value: Box<dyn SignalEvaluator>) -> Self {
        Self(value)
    }
}
