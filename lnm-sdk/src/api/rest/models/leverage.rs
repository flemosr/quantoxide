use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom, fmt};

use super::{Margin, Price, Quantity, error::LeverageValidationError, utils};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Leverage(f64);

impl Leverage {
    pub fn into_f64(self) -> f64 {
        self.into()
    }

    pub fn try_calculate(
        quantity: Quantity,
        margin: Margin,
        price: Price,
    ) -> Result<Self, LeverageValidationError> {
        let leverage_value = quantity.into_u64() as f64 * 100_000_000.
            / (margin.into_u64() as f64 * price.into_f64());
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

    fn try_from(leverage: f64) -> Result<Self, Self::Error> {
        if !leverage.is_finite() {
            return Err(LeverageValidationError::NotFinite);
        }

        if leverage < 1.0 {
            return Err(LeverageValidationError::TooLow);
        }

        if leverage > 100.0 {
            return Err(LeverageValidationError::TooHigh);
        }

        Ok(Leverage(leverage))
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
        self.partial_cmp(other).expect("`Leverage` must be finite")
    }
}

impl fmt::Display for Leverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.0)
    }
}

impl Serialize for Leverage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        utils::float_without_decimal::serialize(&self.0, serializer)
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
