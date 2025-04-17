use async_trait::async_trait;
use std::fmt;

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

#[derive(Debug, Clone, PartialEq)]
pub enum SignalAction {
    Long { min_price: f64, max_price: f64 },
    Neutral,
    Short { min_price: f64, max_price: f64 },
}

#[async_trait]
pub trait SignalEvaluator: Send + Sync {
    fn name(&self) -> &SignalName;

    fn context_window_secs(&self) -> usize;

    async fn evaluate(
        &self,
        entries: &[PriceHistoryEntryLOCF],
    ) -> std::result::Result<Option<SignalAction>, Box<dyn std::error::Error>>;
}
