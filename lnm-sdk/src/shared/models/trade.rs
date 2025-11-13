use std::fmt;

use serde::{Deserialize, Serialize};

/// The side of a trade position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl fmt::Display for TradeSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSide::Buy => "Buy".fmt(f),
            TradeSide::Sell => "Sell".fmt(f),
        }
    }
}
