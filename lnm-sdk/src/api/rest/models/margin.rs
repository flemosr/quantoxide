use serde::{Deserialize, Serialize, de};
use std::{convert::TryFrom, fmt, ops::Add};

use super::{Leverage, Price, Quantity, SATS_PER_BTC, error::MarginValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Margin(u64);

impl Margin {
    pub const MIN: Self = Self(1);

    pub fn into_u64(self) -> u64 {
        self.into()
    }

    pub fn into_i64(self) -> i64 {
        self.into()
    }

    pub fn into_f64(self) -> f64 {
        self.into()
    }

    pub fn try_calculate(
        quantity: Quantity,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, MarginValidationError> {
        let margin =
            quantity.into_f64() * (SATS_PER_BTC / (price.into_f64() * leverage.into_f64()));
        Self::try_from(margin.ceil() as u64)
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

impl From<Margin> for i64 {
    fn from(value: Margin) -> Self {
        value.0 as i64
    }
}

impl From<Margin> for f64 {
    fn from(value: Margin) -> Self {
        value.0 as f64
    }
}

impl TryFrom<u64> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(MarginValidationError::TooLow);
        }

        Ok(Self(value as u64))
    }
}

impl TryFrom<i32> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl fmt::Display for Margin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
        assert_eq!(margin.into_u64(), 5264);

        let leverage = Leverage::try_from(2.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 2632);

        let leverage = Leverage::try_from(50.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 106);

        let leverage = Leverage::try_from(100.0).unwrap();
        let margin = Margin::try_calculate(quantity, price, leverage).unwrap();
        assert_eq!(margin.into_u64(), 53);
    }
}
