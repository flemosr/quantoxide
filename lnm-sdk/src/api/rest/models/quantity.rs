use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{Leverage, Margin, Price, error::QuantityValidationError};

#[derive(Debug, Clone, Copy)]
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

impl PartialEq for Quantity {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Quantity {}

impl PartialOrd for Quantity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Quantity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq<u64> for Quantity {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u64> for Quantity {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialEq<Quantity> for u64 {
    fn eq(&self, other: &Quantity) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Quantity> for u64 {
    fn partial_cmp(&self, other: &Quantity) -> Option<Ordering> {
        Some(self.cmp(&other.0))
    }
}

impl PartialEq<i32> for Quantity {
    fn eq(&self, other: &i32) -> bool {
        if *other < 0 {
            false
        } else {
            self.0 == *other as u64
        }
    }
}

impl PartialOrd<i32> for Quantity {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        if *other < 0 {
            Some(Ordering::Greater)
        } else {
            Some(self.0.cmp(&(*other as u64)))
        }
    }
}

impl PartialEq<Quantity> for i32 {
    fn eq(&self, other: &Quantity) -> bool {
        if *self < 0 {
            false
        } else {
            *self as u64 == other.0
        }
    }
}

impl PartialOrd<Quantity> for i32 {
    fn partial_cmp(&self, other: &Quantity) -> Option<Ordering> {
        if *self < 0 {
            Some(Ordering::Less)
        } else {
            Some((*self as u64).cmp(&other.0))
        }
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
