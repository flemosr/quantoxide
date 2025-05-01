use serde::{Deserialize, Serialize, de};
use std::{convert::TryFrom, ops::Add};

use super::{Leverage, Price, Quantity, error::MarginValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Margin(u64);

impl Margin {
    pub fn into_u64(self) -> u64 {
        self.into()
    }

    pub fn try_calculate(
        quantity: Quantity,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, MarginValidationError> {
        let margin =
            quantity.into_u64() as f64 * (100_000_000. / (price.into_f64() * leverage.into_f64()));
        Self::try_from(margin.floor() as u64)
    }
}

impl Add for Margin {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Margin(self.0 + other.0)
    }
}

impl From<Margin> for u64 {
    fn from(value: Margin) -> Self {
        value.0
    }
}

impl TryFrom<u64> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(MarginValidationError::Zero);
        }

        Ok(Self(value as u64))
    }
}

impl TryFrom<i32> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(MarginValidationError::Negative);
        }

        Self::try_from(value as u64)
    }
}

impl TryFrom<f64> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !value.is_finite() {
            return Err(MarginValidationError::NotFinite);
        }

        if value < 0. {
            return Err(MarginValidationError::Negative);
        }

        if value != value.trunc() {
            return Err(MarginValidationError::NotInteger);
        }

        Ok(Margin(value as u64))
    }
}

impl Serialize for Margin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Margin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let margin_u64 = u64::deserialize(deserializer)?;
        Margin::try_from(margin_u64).map_err(|e| de::Error::custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_margin() {
        let quantity = Quantity::try_from(5).unwrap();
        let price = Price::try_from(95000).unwrap();
        let leverage = Leverage::try_from(1.0).unwrap();

        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 5263);

        let leverage = Leverage::try_from(2.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 2631);

        let leverage = Leverage::try_from(50.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 105);

        let leverage = Leverage::try_from(100.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 52);
    }
}
