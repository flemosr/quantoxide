use std::result;
use thiserror::Error;

use super::simulation::error::SimulationError;

#[derive(Error, Debug)]
pub enum TradeError {
    #[error("[Simulated] {0}")]
    Simulated(#[from] SimulationError),
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TradeError>;
