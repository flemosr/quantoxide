use serde::{Deserialize, Serialize, de};
use std::{convert::TryFrom, fmt};

use super::{
    BoundedPercentage, Leverage, Margin, Price, SATS_PER_BTC, error::QuantityValidationError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(u64);

impl Quantity {
    pub const MIN: Self = Self(1);

    pub const MAX: Self = Self(500_000);

    pub fn into_u64(self) -> u64 {
        self.into()
    }

    pub fn into_f64(self) -> f64 {
        self.into()
    }

    pub fn try_calculate(
        margin: Margin,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.into_u64() as f64 * leverage.into_f64() / SATS_PER_BTC * price.into_f64();
        Self::try_from(qtd.floor() as u64)
    }

    /// Calculates a quantity based on a percentage of the given balance.
    ///
    /// This function converts a balance in sats to USD using the provided market price,
    /// then calculates what quantity corresponds to the specified percentage of that balance.
    ///
    /// # Arguments
    ///
    /// * `balance` - The balance in sats
    /// * `market_price` - The current market price in USD per BTC
    /// * `balance_perc` - The percentage of the balance to use for the calculation
    ///
    /// # Returns
    ///
    /// Returns `Ok(Quantity)` if the calculated quantity is within valid bounds,
    /// or `Err(QuantityValidationError)` if the resulting quantity is too low or too high.
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
        assert_eq!(quantity.into_u64(), 1);

        let margin = Margin::try_from(700).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(2.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity.into_u64(), 1);

        let margin = Margin::try_from(10).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity.into_u64(), 1);

        let margin = Margin::try_from(17).unwrap();
        let price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let quantity = Quantity::try_calculate(margin, price, leverage).unwrap();
        assert_eq!(quantity.into_u64(), 1);
    }
}
