use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{error::PriceValidationError, utils};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Price(f64);

impl Price {
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }

    /// Returns a new valid Price after applying a percentage change to the current price.
    ///
    /// The percentage should be provided as a decimal. For example:
    /// - `-0.05` for a 5% discount (resulting in 0.95 * original price)
    /// - `0.10` for a 10% increase (resulting in 1.10 * original price)
    ///
    /// # Parameters
    /// - `percentage`: The percentage change to apply (must be > -1.0)
    ///
    /// # Returns
    /// A Result containing either the new valid Price or a PriceValidationError
    pub fn apply_change(&self, percentage: f64) -> Result<Self, PriceValidationError> {
        if percentage <= -1.0 {
            return Err(PriceValidationError::InvalidPercentage);
        }

        let target_price = self.0 * (1.0 + percentage);

        let nearest_valid_price = (target_price * 2.0).round() / 2.0;

        Price::try_from(nearest_valid_price)
    }
}

impl From<Price> for f64 {
    fn from(value: Price) -> f64 {
        value.0
    }
}

impl TryFrom<f64> for Price {
    type Error = PriceValidationError;

    fn try_from(price: f64) -> Result<Self, Self::Error> {
        if !price.is_finite() {
            return Err(PriceValidationError::NotFinite);
        }

        if price <= 0.0 {
            return Err(PriceValidationError::NotPositive);
        }

        if (price * 2.0).round() != price * 2.0 {
            return Err(PriceValidationError::NotMultipleOfTick);
        }

        Ok(Price(price))
    }
}

impl TryFrom<i32> for Price {
    type Error = PriceValidationError;

    fn try_from(price: i32) -> Result<Self, Self::Error> {
        Self::try_from(price as f64)
    }
}

impl Eq for Price {}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

impl Serialize for Price {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        utils::float_without_decimal::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Price {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let price_f64 = f64::deserialize(deserializer)?;
        Price::try_from(price_f64).map_err(|e| de::Error::custom(e.to_string()))
    }
}
