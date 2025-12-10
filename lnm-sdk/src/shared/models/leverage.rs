use std::{cmp::Ordering, convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use super::{
    SATS_PER_BTC, error::LeverageValidationError, margin::Margin, price::Price, quantity::Quantity,
    serde_util,
};

/// A validated leverage value for trading positions.
///
/// Leverage represents the multiplier applied to a trader's margin to determine the position size.
/// This type ensures that leverage values are within acceptable bounds (1x to 100x) and can be
/// safely used when trading futures.
///
/// Leverage values must be:
/// + Greater than or equal to [`Leverage::MIN`] (1x)
/// + Less than or equal to [`Leverage::MAX`] (100x)
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v3::models::Leverage;
///
/// // Create a leverage value from a float
/// let leverage = Leverage::try_from(10.0).unwrap();
/// assert_eq!(leverage.as_f64(), 10.0);
///
/// // Values outside the valid range will fail
/// assert!(Leverage::try_from(0.5).is_err());
/// assert!(Leverage::try_from(150.0).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leverage(f64);

impl Leverage {
    /// The minimum allowed leverage value (1x).
    pub const MIN: Self = Self(1.);

    /// The maximum allowed leverage value (100x).
    pub const MAX: Self = Self(100.);

    /// Creates a `Leverage` by bounding the given value to the valid range.
    ///
    /// This method bounds the input to the range ([Leverage::MIN], [Leverage::MAX]).
    /// It should be used to ensure a valid `Leverage` without error handling.
    ///
    /// **Note:** In order to check whether a value is a valid leverage and receive an error for
    /// invalid values, use [`Leverage::try_from`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Leverage;
    ///
    /// // Values within range are preserved
    /// let l = Leverage::bounded(25.0);
    /// assert_eq!(l.as_f64(), 25.0);
    ///
    /// // Values below minimum are bounded to MIN
    /// let l = Leverage::bounded(0.5);
    /// assert_eq!(l, Leverage::MIN);
    ///
    /// // Values above maximum are bounded to MAX
    /// let l = Leverage::bounded(150.0);
    /// assert_eq!(l, Leverage::MAX);
    /// ```
    pub fn bounded<T>(value: T) -> Self
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let clamped = as_f64.clamp(Self::MIN.0, Self::MAX.0);

        Self(clamped)
    }

    /// Returns the leverage value as its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Leverage;
    ///
    /// let leverage = Leverage::try_from(25.0).unwrap();
    /// assert_eq!(leverage.as_f64(), 25.0);
    /// ```
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// Calculates leverage from quantity (USD), margin (sats), and price (BTC/USD).
    ///
    /// The leverage is calculated using the formula:
    ///
    /// leverage = (quantity * SATS_PER_BTC) / (margin * price)
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::{Leverage, Quantity, Margin, Price};
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap(); // Quantity in USD
    /// let margin = Margin::try_from(20_000).unwrap(); // Margin in sats
    /// let price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    ///
    /// let leverage = Leverage::try_calculate(quantity, margin, price).unwrap();
    /// ```
    pub fn try_calculate(
        quantity: Quantity,
        margin: Margin,
        price: Price,
    ) -> Result<Self, LeverageValidationError> {
        let leverage_value = quantity.as_f64() * SATS_PER_BTC / (margin.as_f64() * price.as_f64());

        Self::try_from(leverage_value)
    }
}

impl From<Leverage> for f64 {
    fn from(value: Leverage) -> f64 {
        value.0
    }
}

impl TryFrom<f64> for Leverage {
    type Error = LeverageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(LeverageValidationError::TooLow { value });
        }

        if value > Self::MAX.0 {
            return Err(LeverageValidationError::TooHigh { value });
        }

        Ok(Leverage(value))
    }
}

impl TryFrom<i32> for Leverage {
    type Error = LeverageValidationError;

    fn try_from(leverage: i32) -> Result<Self, Self::Error> {
        Self::try_from(leverage as f64)
    }
}

impl Eq for Leverage {}

impl Ord for Leverage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("`Leverage` must be finite")
    }
}

impl PartialOrd for Leverage {
    fn partial_cmp(&self, other: &Leverage) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Leverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Leverage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_util::float_without_decimal::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Leverage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let leverage_f64 = f64::deserialize(deserializer)?;
        Leverage::try_from(leverage_f64).map_err(|e| de::Error::custom(e.to_string()))
    }
}
