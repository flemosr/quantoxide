use serde::{Deserialize, Serialize, de};
use std::convert::TryFrom;

use super::{Leverage, Margin, Price, error::QuantityValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(u64);

impl Quantity {
    pub fn into_u64(self) -> u64 {
        self.into()
    }

    pub fn try_calculate(
        margin: Margin,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, QuantityValidationError> {
        let qtd = margin.into_u64() as f64 / 100000000. * price.into_f64() * leverage.into_f64();
        Self::try_from(qtd)
    }
}

impl From<Quantity> for u64 {
    fn from(value: Quantity) -> Self {
        value.0
    }
}

impl TryFrom<u64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: u64) -> Result<Self, Self::Error> {
        if quantity < 1 {
            return Err(QuantityValidationError::TooLow);
        }

        if quantity > 500_000 {
            return Err(QuantityValidationError::TooHigh);
        }

        Ok(Quantity(quantity))
    }
}

impl TryFrom<i32> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: i32) -> Result<Self, Self::Error> {
        if quantity < 0 {
            return Err(QuantityValidationError::NotPositive);
        }

        Self::try_from(quantity as u64)
    }
}

impl TryFrom<f64> for Quantity {
    type Error = QuantityValidationError;

    fn try_from(quantity: f64) -> Result<Self, Self::Error> {
        if !quantity.is_finite() {
            return Err(QuantityValidationError::NotFinite);
        }

        if quantity < 0. {
            return Err(QuantityValidationError::NotPositive);
        }

        let quantity_u64 = quantity as u64;

        Self::try_from(quantity_u64)
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
