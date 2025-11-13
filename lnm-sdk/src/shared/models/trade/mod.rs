use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    error::QuantityValidationError, leverage::Leverage, margin::Margin, price::Price,
    quantity::Quantity,
};

pub mod util;

/// The side of a trade position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
#[serde(rename_all = "lowercase")]
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

/// The size specification for a trade position.
///
/// Trade size can be specified either as a [`Quantity`] (notional value in USD) or as [`Margin`]
/// (collateral in satoshis). The API will calculate the corresponding value based on the provided
/// price and leverage.
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::{TradeSize, Quantity, Margin};
///
/// // Specify size by quantity (USD notional value)
/// let size_by_quantity = TradeSize::from(Quantity::try_from(1_000).unwrap());
///
/// // Specify size by margin (satoshis collateral)
/// let size_by_margin = TradeSize::from(Margin::try_from(10_000).unwrap());
/// ```
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSize {
    #[serde(rename = "quantity")]
    Quantity(Quantity),
    #[serde(rename = "margin")]
    Margin(Margin),
}

impl TradeSize {
    /// Converts the trade size to both quantity and margin values.
    ///
    /// Calculates the corresponding quantity and margin based on the provided price and leverage.
    /// If the size is specified as margin, the quantity is calculated. If specified as quantity,
    /// the margin is calculated.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::{TradeSize, Quantity, Price, Leverage};
    ///
    /// let size = TradeSize::from(Quantity::try_from(1_000).unwrap());
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let leverage = Leverage::try_from(10.0).unwrap();
    ///
    /// let (quantity, margin) = size.to_quantity_and_margin(price, leverage).unwrap();
    /// ```
    pub fn to_quantity_and_margin(
        &self,
        price: Price,
        leverage: Leverage,
    ) -> Result<(Quantity, Margin), QuantityValidationError> {
        match self {
            TradeSize::Margin(margin) => {
                let quantity = Quantity::try_calculate(*margin, price, leverage)?;
                Ok((quantity, *margin))
            }
            TradeSize::Quantity(quantity) => {
                let margin = Margin::calculate(*quantity, price, leverage);
                Ok((*quantity, margin))
            }
        }
    }
}

impl From<Quantity> for TradeSize {
    fn from(quantity: Quantity) -> Self {
        TradeSize::Quantity(quantity)
    }
}

impl From<Margin> for TradeSize {
    fn from(margin: Margin) -> Self {
        TradeSize::Margin(margin)
    }
}

impl fmt::Display for TradeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSize::Quantity(quantity) => write!(f, "Quantity({})", quantity),
            TradeSize::Margin(margin) => write!(f, "Margin({})", margin),
        }
    }
}

/// The execution type of a trade.
///
/// Represents whether a trade is executed at market price or at a specific limit price.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
#[serde(rename_all = "lowercase")]
pub enum TradeExecutionType {
    Market,
    Limit,
}

/// The execution specification for a trade order.
///
/// Trades can be executed:
/// + Immediately at market price
/// + At a specific limit price
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::{TradeExecution, Price};
///
/// // Execute immediately at market price
/// let market_execution = TradeExecution::Market;
///
/// // Execute only at or better than the specified price
/// let limit_execution = TradeExecution::Limit(Price::try_from(100_000.0).unwrap());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecution {
    Market,
    Limit(Price),
}

impl TradeExecution {
    /// Returns the execution type without the associated price data.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::{TradeExecution, TradeExecutionType, Price};
    ///
    /// let market_execution = TradeExecution::Market;
    /// assert!(matches!(market_execution.to_type(), TradeExecutionType::Market));
    ///
    /// let limit_execution = TradeExecution::Limit(Price::try_from(100_000.0).unwrap());
    /// assert!(matches!(limit_execution.to_type(), TradeExecutionType::Limit));
    /// ```
    pub fn to_type(&self) -> TradeExecutionType {
        match self {
            TradeExecution::Market => TradeExecutionType::Market,
            TradeExecution::Limit(_) => TradeExecutionType::Limit,
        }
    }
}

impl From<Price> for TradeExecution {
    fn from(price: Price) -> Self {
        Self::Limit(price)
    }
}

/// The lifecycle status of a trade.
pub enum TradeStatus {
    Open,
    Running,
    Closed,
}

impl TradeStatus {
    /// Returns the status as a string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeStatus::Open => "open",
            TradeStatus::Running => "running",
            TradeStatus::Closed => "closed",
        }
    }

    /// Converts the status to an owned String.
    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}
