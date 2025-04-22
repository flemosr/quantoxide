use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{error::LeverageValidationError, utils};

#[derive(Debug, Clone, Copy)]
pub struct Leverage(f64);

impl Leverage {
    pub fn into_f64(self) -> f64 {
        self.0
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

impl PartialEq for Leverage {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Leverage {}

impl PartialOrd for Leverage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for Leverage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq<f64> for Leverage {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for Leverage {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<Leverage> for f64 {
    fn eq(&self, other: &Leverage) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Leverage> for f64 {
    fn partial_cmp(&self, other: &Leverage) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialEq<i32> for Leverage {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<i32> for Leverage {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<Leverage> for i32 {
    fn eq(&self, other: &Leverage) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<Leverage> for i32 {
    fn partial_cmp(&self, other: &Leverage) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
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
