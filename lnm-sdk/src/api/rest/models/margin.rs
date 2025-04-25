use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{Leverage, Price, Quantity, error::MarginValidationError};

#[derive(Debug, Clone, Copy)]
pub struct Margin(u64);

impl Margin {
    pub fn into_u64(self) -> u64 {
        u64::from(self)
    }

    pub fn try_calculate(
        quantity: Quantity,
        price: Price,
        leverage: Leverage,
    ) -> Result<Self, MarginValidationError> {
        let margin =
            quantity.into_u64() as f64 / (price.into_f64() * leverage.into_f64()) * 100000000.;
        Self::try_from(margin.ceil())
    }
}

impl From<Margin> for u64 {
    fn from(value: Margin) -> Self {
        value.0
    }
}

impl TryFrom<u64> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: u64) -> Result<Self, Self::Error> {
        if margin < 1 {
            return Err(MarginValidationError::TooLow);
        }

        Ok(Margin(margin))
    }
}

impl TryFrom<i32> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: i32) -> Result<Self, Self::Error> {
        if margin < 0 {
            return Err(MarginValidationError::NotPositive);
        }

        Self::try_from(margin as u64)
    }
}

impl TryFrom<f64> for Margin {
    type Error = MarginValidationError;

    fn try_from(margin: f64) -> Result<Self, Self::Error> {
        if !margin.is_finite() {
            return Err(MarginValidationError::NotFinite);
        }

        if margin < 0. {
            return Err(MarginValidationError::NotPositive);
        }

        Ok(Margin(margin as u64))
    }
}

impl PartialEq for Margin {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Margin {}

impl PartialOrd for Margin {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Margin {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq<u64> for Margin {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u64> for Margin {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialEq<Margin> for u64 {
    fn eq(&self, other: &Margin) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Margin> for u64 {
    fn partial_cmp(&self, other: &Margin) -> Option<Ordering> {
        Some(self.cmp(&other.0))
    }
}

impl PartialEq<i32> for Margin {
    fn eq(&self, other: &i32) -> bool {
        if *other < 0 {
            false
        } else {
            self.0 == *other as u64
        }
    }
}

impl PartialOrd<i32> for Margin {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        if *other < 0 {
            Some(Ordering::Greater)
        } else {
            Some(self.0.cmp(&(*other as u64)))
        }
    }
}

impl PartialEq<Margin> for i32 {
    fn eq(&self, other: &Margin) -> bool {
        if *self < 0 {
            false
        } else {
            *self as u64 == other.0
        }
    }
}

impl PartialOrd<Margin> for i32 {
    fn partial_cmp(&self, other: &Margin) -> Option<Ordering> {
        if *self < 0 {
            Some(Ordering::Less)
        } else {
            Some((*self as u64).cmp(&other.0))
        }
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
