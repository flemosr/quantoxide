use std::{convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use super::{
    SATS_PER_BTC,
    error::QuantityValidationError,
    leverage::Leverage,
    margin::Margin,
    price::{PercentageCapped, Price},
};

/// A validated quantity value denominated in USD.
///
/// Quantity represents the notional value of a trading position in USD.
/// This type ensures that quantity values are within acceptable bounds and can be safely used when
/// trading futures.
///
/// Quantity values must be:
/// + Integer values (whole USD amounts)
/// + Greater than or equal to [`Quantity::MIN`] (1 USD)
/// + Less than or equal to [`Quantity::MAX`] (500,000 USD)
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v3::models::Quantity;
///
/// // Create a quantity value from USD amount
/// let quantity = Quantity::try_from(1_000).unwrap();
/// assert_eq!(quantity.as_u64(), 1_000);
///
/// // Values outside the valid range will fail
/// assert!(Quantity::try_from(0).is_err());
/// assert!(Quantity::try_from(600_000).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(u64);

impl Quantity {
    /// The minimum allowed quantity value (1 USD).
    pub const MIN: Self = Self(1);

    /// The maximum allowed quantity value (500,000 USD).
    pub const MAX: Self = Self(500_000);

    /// Creates a `Quantity` by rounding and bounding the given value to the valid range.
    ///
    /// This method rounds the input to the nearest integer and bounds it to the range
    /// ([Quantity::MIN], [Quantity::MAX]).
    /// It should be used to ensure a valid `Quantity` without error handling.
    ///
    /// **Note:** In order to check whether a value is a valid quantity and receive an error for
    /// invalid values, use [`Quantity::try_from`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Quantity;
    ///
    /// // Values within range are rounded
    /// let q = Quantity::bounded(1_234.7);
    /// assert_eq!(q.as_u64(), 1_235);
    ///
    /// // Values below minimum are bounded to MIN
    /// let q = Quantity::bounded(-1);
    /// assert_eq!(q, Quantity::MIN);
    ///
    /// // Values above maximum are bounded to MAX
    /// let q = Quantity::bounded(600_000);
    /// assert_eq!(q, Quantity::MAX);
    /// ```
    pub fn bounded<T>(value: T) -> Self
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let rounded = as_f64.round().max(0.0) as u64;
        let clamped = rounded.clamp(Self::MIN.0, Self::MAX.0);

        Self(clamped)
    }

    /// Returns the quantity value as its underlying `u64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Quantity;
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// assert_eq!(quantity.as_u64(), 1_000);
    /// ```
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the quantity value as a `f64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Quantity;
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// assert_eq!(quantity.as_f64(), 1_000.0);
    /// ```
    pub fn as_f64(&self) -> f64 {
        self.0 as f64
    }

    /// Calculates quantity (USD) from margin (sats), price (BTC/USD), and leverage.
    ///
    /// The quantity is calculated using the formula:
    ///
    /// quantity = (margin * leverage * price) / SATS_PER_BTC
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::{Quantity, Margin, Price, Leverage};
    ///
    /// let margin = Margin::try_from(10_000).unwrap(); // Margin in sats
    /// let price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    /// let leverage = Leverage::try_from(10.0).unwrap();
    ///
    /// let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
    ///
    /// assert_eq!(quantity.as_u64(), 100); // 100 [USD]
    /// ```
    pub fn try_calculate(
        margin: Margin,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.as_f64() * leverage.as_f64() * price.as_f64() / SATS_PER_BTC;

        Self::try_from(qtd.floor() as u64)
    }

    /// Calculates quantity from a percentage of a given balance.
    ///
    /// Converts a balance in satoshis to USD using the provided market price, then calculates the
    /// quantity corresponding to the specified percentage of that balance.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::{Quantity, Price, PercentageCapped};
    ///
    /// let balance = 10_000_000; // In sats
    /// let market_price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    /// let balance_perc = PercentageCapped::try_from(10.0).unwrap(); // 10%
    ///
    /// let quantity = Quantity::try_from_balance_perc(
    ///     balance,
    ///     market_price,
    ///     balance_perc
    /// ).unwrap();
    ///
    /// assert_eq!(quantity.as_u64(), 1_000); // 1_000 [USD]
    /// ```
    pub fn try_from_balance_perc(
        balance: u64,
        market_price: Price,
        balance_perc: PercentageCapped,
    ) -> Result<Self, QuantityValidationError> {
        let balance_usd = balance as f64 * market_price.as_f64() / SATS_PER_BTC;
        let quantity_target = balance_usd * balance_perc.as_f64() / 100.;

        Quantity::try_from(quantity_target.floor())
    }
}

impl From<Quantity> for u64 {
    fn from(value: Quantity) -> Self {
        value.0
    }
}

impl From<Quantity> for f64 {
    fn from(value: Quantity) -> Self {
        value.0 as f64
    }
}

impl TryFrom<u8> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u16> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(QuantityValidationError::TooLow { value });
        }

        if value > Self::MAX.0 {
            return Err(QuantityValidationError::TooHigh { value });
        }

        Ok(Quantity(value))
    }
}

impl TryFrom<i8> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<i16> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<i32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: i32) -> Result<Self, Self::Error> {
        Self::try_from(quantity.max(0) as u64)
    }
}

impl TryFrom<i64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<usize> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<isize> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: isize) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl TryFrom<f32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl TryFrom<f64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.fract() != 0.0 {
            return Err(QuantityValidationError::NotAnInteger { value });
        }

        Self::try_from(value.max(0.) as u64)
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Quantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let quantity_u64 = u64::deserialize(deserializer)?;
        Quantity::try_from(quantity_u64).map_err(|e| de::Error::custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_quantity() {
        let margin = Margin::try_from(1_000).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(1.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity, Quantity::MIN);

        let margin = Margin::try_from(700).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(2.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity, Quantity::MIN);

        let margin = Margin::try_from(10).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity, Quantity::MIN);

        let margin = Margin::try_from(5_000_000).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity, Quantity::MAX);

        let margin = Margin::try_from(9).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity_validation_error = Quantity::try_calculate(margin, price, leverage)
            .err()
            .unwrap();
        assert!(matches!(
            quantity_validation_error,
            QuantityValidationError::TooLow { value: _ }
        ));

        let margin = Margin::try_from(5_001_000).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity_validation_error = Quantity::try_calculate(margin, price, leverage)
            .err()
            .unwrap();
        assert!(matches!(
            quantity_validation_error,
            QuantityValidationError::TooHigh { value: _ }
        ));
    }
}
