use std::{convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use crate::{
    api_v3::models::Leverage,
    shared::models::{
        SATS_PER_BTC, error::LeverageValidationError, margin::Margin, price::Price,
        quantity::Quantity,
    },
};

/// A validated leverage value for futures cross positions.
///
/// Leverage represents the multiplier applied to the position margin to determine the position size
/// (quantity).
/// This type ensures that leverage can be safely used with futures cross orders and positions.
///
/// Leverage values must be:
/// + Integers
/// + Greater than or equal to [`CrossLeverage::MIN`] (1x)
/// + Less than or equal to [`CrossLeverage::MAX`] (100x)
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v3::models::CrossLeverage;
///
/// // Create a leverage value from an integer
/// let leverage = CrossLeverage::try_from(10).unwrap();
/// assert_eq!(leverage.into_u64(), 10);
///
/// // Create a leverage value from a float
/// let leverage = CrossLeverage::try_from(10.9).unwrap();
/// assert_eq!(leverage.into_u64(), 10);
///
/// // Values outside the valid range will fail
/// assert!(CrossLeverage::try_from(0.9).is_err());
/// assert!(CrossLeverage::try_from(101).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrossLeverage(u64);

impl CrossLeverage {
    /// The minimum allowed leverage value (1x).
    pub const MIN: Self = Self(1);

    /// The maximum allowed leverage value (100x).
    pub const MAX: Self = Self(100);

    /// Converts the leverage value to its underlying `u64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::CrossLeverage;
    ///
    /// let leverage = CrossLeverage::try_from(25.0).unwrap();
    /// assert_eq!(leverage.into_u64(), 25);
    /// ```
    pub fn into_u64(self) -> u64 {
        self.into()
    }

    /// Calculates the rounded leverage from quantity (USD), margin (sats), and price (BTC/USD).
    ///
    /// The leverage is calculated using the formula:
    ///
    /// leverage = (quantity * SATS_PER_BTC) / (margin * price)
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::{CrossLeverage, Quantity, Margin, Price};
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap(); // Quantity in USD
    /// let margin = Margin::try_from(20_000).unwrap(); // Margin in sats
    /// let price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    ///
    /// let leverage = CrossLeverage::try_calculate_rounded(quantity, margin, price).unwrap();
    /// ```
    pub fn try_calculate_rounded(
        quantity: Quantity,
        margin: Margin,
        price: Price,
    ) -> Result<Self, LeverageValidationError> {
        let leverage_value =
            quantity.into_f64() * SATS_PER_BTC / (margin.into_f64() * price.into_f64());

        Self::try_from(leverage_value.round())
    }
}

impl From<CrossLeverage> for u64 {
    fn from(value: CrossLeverage) -> u64 {
        value.0
    }
}

impl From<CrossLeverage> for Leverage {
    fn from(value: CrossLeverage) -> Leverage {
        Leverage::try_from(value.0 as f64).expect("Must be a valid `Leverage`")
    }
}

impl TryFrom<u64> for CrossLeverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: u64) -> Result<Self, Self::Error> {
        if leverage < Self::MIN.0 {
            return Err(LeverageValidationError::TooLow);
        }

        if leverage > Self::MAX.0 {
            return Err(LeverageValidationError::TooHigh);
        }

        Ok(CrossLeverage(leverage))
    }
}

impl TryFrom<i32> for CrossLeverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: i32) -> Result<Self, Self::Error> {
        Self::try_from(leverage.max(0) as u64)
    }
}

impl TryFrom<f64> for CrossLeverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: f64) -> Result<Self, Self::Error> {
        Self::try_from(leverage.max(0.) as u64)
    }
}

impl fmt::Display for CrossLeverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for CrossLeverage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for CrossLeverage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let leverage_u64 = u64::deserialize(deserializer)?;
        CrossLeverage::try_from(leverage_u64).map_err(|e| de::Error::custom(e.to_string()))
    }
}
