use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{
    error::{BoundedPercentageValidationError, PriceValidationError},
    utils,
};

/// Represents a percentage value that is constrained within a specific range.
///
/// This struct wraps an f32 value that must be:
/// - Greater than or equal to 0.1%
/// - Less than or equal to 99.9%
///
/// This bounded range makes it suitable for percentage calculations where both
/// minimum and maximum limits are required.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BoundedPercentage(f32);

impl TryFrom<f32> for BoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value < 0.1 {
            return Err(BoundedPercentageValidationError::BelowMinimum);
        }
        if value > 99.9 {
            return Err(BoundedPercentageValidationError::AboveMaximum);
        }
        if !value.is_finite() {
            return Err(BoundedPercentageValidationError::NotFinite);
        }
        Ok(Self(value))
    }
}

impl From<BoundedPercentage> for f32 {
    fn from(perc: BoundedPercentage) -> f32 {
        perc.0
    }
}

impl Eq for BoundedPercentage {}

impl Ord for BoundedPercentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("`BoundedPercentage` must be finite")
    }
}

/// Represents a percentage value that is only constrained by a lower bound.
///
/// This struct wraps an f32 value that must be:
/// - Greater than or equal to 0.1%
/// - Finite (not infinity)
///
/// This type is suitable for percentage calculations where only a minimum
/// threshold is needed, with no practical upper limit other than it must be a
/// finite value.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct LowerBoundedPercentage(f32);

impl TryFrom<f32> for LowerBoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value < 0.1 {
            return Err(BoundedPercentageValidationError::BelowMinimum);
        }
        if !value.is_finite() {
            return Err(BoundedPercentageValidationError::NotFinite);
        }
        Ok(Self(value))
    }
}

impl From<LowerBoundedPercentage> for f32 {
    fn from(perc: LowerBoundedPercentage) -> f32 {
        perc.0
    }
}

impl Eq for LowerBoundedPercentage {}

impl Ord for LowerBoundedPercentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("`LowerBoundedPercentage` must be finite")
    }
}

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
