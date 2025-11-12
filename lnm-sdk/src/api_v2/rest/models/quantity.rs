use std::{convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use super::{
    SATS_PER_BTC, error::QuantityValidationError, leverage::Leverage, margin::Margin,
    price::BoundedPercentage, price::Price,
};

/// A validated quantity value denominated in USD.
///
/// Quantity represents the notional value of a trading position in USD.
/// This type ensures that quantity values are within acceptable bounds and can be safely used
/// with [`Trade`] implementations.
///
/// Quantity values must be:
/// + Integer values (whole USD amounts)
/// + Greater than or equal to [`Quantity::MIN`] (1 USD)
/// + Less than or equal to [`Quantity::MAX`] (500,000 USD)
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::Quantity;
///
/// // Create a quantity value from USD amount
/// let quantity = Quantity::try_from(1_000).unwrap();
/// assert_eq!(quantity.into_u64(), 1_000);
///
/// // Values outside the valid range will fail
/// assert!(Quantity::try_from(0).is_err());
/// assert!(Quantity::try_from(600_000).is_err());
/// ```
///
/// [`Trade`]: crate::models::Trade
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(u64);

impl Quantity {
    /// The minimum allowed quantity value (1 USD).
    pub const MIN: Self = Self(1);

    /// The maximum allowed quantity value (500,000 USD).
    pub const MAX: Self = Self(500_000);

    /// Converts the quantity value to its underlying `u64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Quantity;
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// assert_eq!(quantity.into_u64(), 1_000);
    /// ```
    pub fn into_u64(self) -> u64 {
        self.into()
    }

    /// Converts the quantity value to `f64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Quantity;
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// assert_eq!(quantity.into_f64(), 1_000.0);
    /// ```
    pub fn into_f64(self) -> f64 {
        self.into()
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
    /// use lnm_sdk::api_v2::models::{Quantity, Margin, Price, Leverage};
    ///
    /// let margin = Margin::try_from(10_000).unwrap(); // Margin in sats
    /// let price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    /// let leverage = Leverage::try_from(10.0).unwrap();
    ///
    /// let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
    ///
    /// assert_eq!(quantity.into_u64(), 100); // 100 [USD]
    /// ```
    pub fn try_calculate(
        margin: Margin,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.into_f64() * leverage.into_f64() * price.into_f64() / SATS_PER_BTC;

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
    /// use lnm_sdk::api_v2::models::{Quantity, Price, BoundedPercentage};
    ///
    /// let balance = 10_000_000; // In sats
    /// let market_price = Price::try_from(100_000.0).unwrap(); // Price in USD/BTC
    /// let balance_perc = BoundedPercentage::try_from(10.0).unwrap(); // 10%
    ///
    /// let quantity = Quantity::try_from_balance_perc(
    ///     balance,
    ///     market_price,
    ///     balance_perc
    /// ).unwrap();
    ///
    /// assert_eq!(quantity.into_u64(), 1_000); // 1_000 [USD]
    /// ```
    pub fn try_from_balance_perc(
        balance: u64,
        market_price: Price,
        balance_perc: BoundedPercentage,
    ) -> Result<Self, QuantityValidationError> {
        let balance_usd = balance as f64 * market_price.into_f64() / SATS_PER_BTC;
        let quantity_target = balance_usd * balance_perc.into_f64() / 100.;

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

impl TryFrom<u64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: u64) -> Result<Self, Self::Error> {
        if quantity < Self::MIN.0 {
            return Err(QuantityValidationError::TooLow);
        }

        if quantity > Self::MAX.0 {
            return Err(QuantityValidationError::TooHigh);
        }

        Ok(Quantity(quantity))
    }
}

impl TryFrom<i32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: i32) -> Result<Self, Self::Error> {
        Self::try_from(quantity.max(0) as u64)
    }
}

impl TryFrom<f64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: f64) -> Result<Self, Self::Error> {
        Self::try_from(quantity.max(0.) as u64)
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
            QuantityValidationError::TooLow
        ));

        let margin = Margin::try_from(5_001_000).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity_validation_error = Quantity::try_calculate(margin, price, leverage)
            .err()
            .unwrap();
        assert!(matches!(
            quantity_validation_error,
            QuantityValidationError::TooHigh
        ));
    }
}
