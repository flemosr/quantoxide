use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom, fmt};

use super::{
    error::{BoundedPercentageValidationError, PriceValidationError},
    utils,
};

/// Represents a percentage value that is constrained within a specific range.
///
/// This struct wraps an f64 value that must be:
/// - Greater than or equal to 0.1%
/// - Less than or equal to 99.9%
///
/// This bounded range makes it suitable for percentage calculations where both
/// minimum and maximum limits are required.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BoundedPercentage(f64);

impl BoundedPercentage {
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }
}

impl TryFrom<f64> for BoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
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

impl TryFrom<i32> for BoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl From<BoundedPercentage> for f64 {
    fn from(perc: BoundedPercentage) -> f64 {
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
/// This struct wraps an f64 value that must be:
/// - Greater than or equal to 0.1%
/// - Finite (not infinity)
///
/// This type is suitable for percentage calculations where only a minimum
/// threshold is needed, with no practical upper limit other than it must be a
/// finite value.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct LowerBoundedPercentage(f64);

impl LowerBoundedPercentage {
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }
}

impl TryFrom<f64> for LowerBoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < 0.1 {
            return Err(BoundedPercentageValidationError::BelowMinimum);
        }
        if !value.is_finite() {
            return Err(BoundedPercentageValidationError::NotFinite);
        }
        Ok(Self(value))
    }
}

impl TryFrom<i32> for LowerBoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl From<LowerBoundedPercentage> for f64 {
    fn from(perc: LowerBoundedPercentage) -> f64 {
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

impl From<BoundedPercentage> for LowerBoundedPercentage {
    fn from(bounded: BoundedPercentage) -> Self {
        LowerBoundedPercentage(bounded.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Price(f64);

impl Price {
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }

    pub fn round_down(value: f64) -> Result<Self, PriceValidationError> {
        let round_down = (value * 2.0).floor() / 2.0;
        Price::try_from(round_down)
    }

    pub fn round_up(value: f64) -> Result<Self, PriceValidationError> {
        let round_up = (value * 2.0).ceil() / 2.0;
        Price::try_from(round_up)
    }

    pub fn round(value: f64) -> Result<Self, PriceValidationError> {
        let round = (value * 2.0).round() / 2.0;
        Price::try_from(round)
    }

    /// Applies a discount percentage to the current price.
    ///
    /// # Parameters
    /// - `percentage`: The discount percentage to apply (between 0.1% and 99.9%)
    ///
    /// # Returns
    /// A Result containing either the new discounted Price or a PriceValidationError
    pub fn apply_discount(
        &self,
        percentage: BoundedPercentage,
    ) -> Result<Self, PriceValidationError> {
        let discount_factor = 1.0 - (f64::from(percentage) / 100.0);
        let target_price = self.0 * discount_factor;

        let nearest_rounded_price = (target_price * 2.0).round() / 2.0;

        Price::try_from(nearest_rounded_price)
    }

    /// Applies a gain percentage to the current price.
    ///
    /// # Parameters
    /// - `percentage`: The gain percentage to apply (minimum 0.1%, no upper bound)
    ///
    /// # Returns
    /// A Result containing either the new increased Price or a PriceValidationError
    pub fn apply_gain(
        &self,
        percentage: LowerBoundedPercentage,
    ) -> Result<Self, PriceValidationError> {
        let gain_factor = 1.0 + (f64::from(percentage) / 100.0);
        let target_price = self.0 * gain_factor;

        let nearest_rounded_price = (target_price * 2.0).round() / 2.0;

        Price::try_from(nearest_rounded_price)
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

        if price < 1.0 {
            return Err(PriceValidationError::AtLeastOne);
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
        self.partial_cmp(other).expect("`Price` must be finite")
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
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
