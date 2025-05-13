use std::result;
use thiserror::Error;

use super::{live::error::LiveError, simulation::error::SimulationError};

#[derive(Error, Debug)]
pub enum TradeError {
    #[error("[Simulated] {0}")]
    Simulated(#[from] SimulationError),

    #[error("[Live] {0}")]
    Live(#[from] LiveError),

    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TradeError>;
