use std::{cmp::Ordering, convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use super::{
    error::{
        BoundedPercentageValidationError, LowerBoundedPercentageValidationError,
        PriceValidationError,
    },
    serde_util,
};

/// A validated percentage value constrained within a specific range.
///
/// Percentage values must be:
/// + Greater than or equal to [`BoundedPercentage::MIN`] (0.1%)
/// + Less than or equal to [`BoundedPercentage::MAX`] (99.9%)
///
/// This bounded range makes it suitable for percentage calculations where both
/// minimum and maximum limits are required, such as discount calculations.
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::BoundedPercentage;
///
/// // Create a bounded percentage value
/// let percentage = BoundedPercentage::try_from(50.0).unwrap();
/// assert_eq!(percentage.into_f64(), 50.0);
///
/// // Values outside the valid range will fail
/// assert!(BoundedPercentage::try_from(0.05).is_err());
/// assert!(BoundedPercentage::try_from(100.0).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BoundedPercentage(f64);

impl BoundedPercentage {
    /// The minimum allowed percentage value (0.1%).
    pub const MIN: Self = Self(0.1);

    /// The maximum allowed percentage value (99.9%).
    pub const MAX: Self = Self(99.9);

    /// Converts the percentage value to its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::BoundedPercentage;
    ///
    /// let percentage = BoundedPercentage::try_from(25.5).unwrap();
    /// assert_eq!(percentage.into_f64(), 25.5);
    /// ```
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }
}

impl TryFrom<f64> for BoundedPercentage {
    type Error = BoundedPercentageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(BoundedPercentageValidationError::BelowMinimum { value });
        }
        if value > Self::MAX.0 {
            return Err(BoundedPercentageValidationError::AboveMaximum { value });
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
    fn from(value: BoundedPercentage) -> f64 {
        value.0
    }
}

impl Eq for BoundedPercentage {}

impl Ord for BoundedPercentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("`BoundedPercentage` must be finite")
    }
}

impl fmt::Display for BoundedPercentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A validated percentage value constrained only by a lower bound.
///
/// Percentage values must be:
/// + Greater than or equal to [`LowerBoundedPercentage::MIN`] (0.1%)
/// + Finite (not infinity)
///
/// This type is suitable for percentage calculations where only a minimum
/// threshold is needed, with no practical upper limit other than it must be a
/// finite value, such as gain calculations.
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::LowerBoundedPercentage;
///
/// // Create a lower-bounded percentage value
/// let percentage = LowerBoundedPercentage::try_from(150.0).unwrap();
/// assert_eq!(percentage.into_f64(), 150.0);
///
/// // Values below the minimum will fail
/// assert!(LowerBoundedPercentage::try_from(0.05).is_err());
///
/// // Non-finite values will fail
/// assert!(LowerBoundedPercentage::try_from(f64::INFINITY).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct LowerBoundedPercentage(f64);

impl LowerBoundedPercentage {
    /// The minimum allowed percentage value (0.1%).
    pub const MIN: Self = Self(0.1);

    /// Converts the percentage value to its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::LowerBoundedPercentage;
    ///
    /// let percentage = LowerBoundedPercentage::try_from(200.0).unwrap();
    /// assert_eq!(percentage.into_f64(), 200.0);
    /// ```
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }
}

impl TryFrom<f64> for LowerBoundedPercentage {
    type Error = LowerBoundedPercentageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(LowerBoundedPercentageValidationError::BelowMinimum { value });
        }
        if !value.is_finite() {
            return Err(LowerBoundedPercentageValidationError::NotFinite);
        }

        Ok(Self(value))
    }
}

impl TryFrom<i32> for LowerBoundedPercentage {
    type Error = LowerBoundedPercentageValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl From<LowerBoundedPercentage> for f64 {
    fn from(value: LowerBoundedPercentage) -> f64 {
        value.0
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
    fn from(value: BoundedPercentage) -> Self {
        Self(value.0)
    }
}

impl fmt::Display for LowerBoundedPercentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A validated price value denominated in USD per BTC.
///
/// Price represents the market price of Bitcoin in USD.
/// This type ensures that price values are within acceptable bounds and conform to the minimum tick
/// size requirement.
///
/// Price values must be:
/// + Greater than or equal to [`Price::MIN`] (1 USD)
/// + Less than or equal to [`Price::MAX`] (100,000,000 USD)
/// + A multiple of [`Price::TICK`] (0.5 USD)
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v2::models::Price;
///
/// // Create a price value from USD amount
/// let price = Price::try_from(100_000.0).unwrap();
/// assert_eq!(price.into_f64(), 100_000.0);
///
/// // Values outside the valid range will fail
/// assert!(Price::try_from(0.5).is_err());
/// assert!(Price::try_from(150_000_000.0).is_err());
///
/// // Values not aligned to the tick size will fail
/// assert!(Price::try_from(100_000.25).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Price(f64);

impl Price {
    /// The minimum allowed price value (1 USD).
    pub const MIN: Self = Self(1.);

    /// The maximum allowed price value (100,000,000 USD).
    pub const MAX: Self = Self(100_000_000.);

    /// The minimum price increment (0.5 USD).
    ///
    /// All valid prices must be a multiple of this tick size.
    pub const TICK: f64 = 0.5;

    /// Converts the price value to its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Price;
    ///
    /// let price = Price::try_from(50_000.0).unwrap();
    /// assert_eq!(price.into_f64(), 50_000.0);
    /// ```
    pub fn into_f64(self) -> f64 {
        f64::from(self)
    }

    /// Rounds a value down to the nearest valid price.
    ///
    /// The value is rounded down to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Price;
    ///
    /// let price = Price::round_down(100_000.8).unwrap();
    /// assert_eq!(price.into_f64(), 100_000.5);
    /// ```
    pub fn round_down(value: f64) -> Result<Self, PriceValidationError> {
        let round_down = (value / Self::TICK).floor() * Self::TICK;

        Self::try_from(round_down)
    }

    /// Rounds a value up to the nearest valid price.
    ///
    /// The value is rounded up to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Price;
    ///
    /// let price = Price::round_up(100_000.2).unwrap();
    /// assert_eq!(price.into_f64(), 100_000.5);
    /// ```
    pub fn round_up(value: f64) -> Result<Self, PriceValidationError> {
        let round_up = (value / Self::TICK).ceil() * Self::TICK;

        Self::try_from(round_up)
    }

    /// Rounds a value to the nearest valid price.
    ///
    /// The value is rounded to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Price;
    ///
    /// let price = Price::round(100_000.6).unwrap();
    /// assert_eq!(price.into_f64(), 100_000.5);
    ///
    /// let price = Price::round(100_000.8).unwrap();
    /// assert_eq!(price.into_f64(), 100_001.0);
    /// ```
    pub fn round(value: f64) -> Result<Self, PriceValidationError> {
        let round = (value / Self::TICK).round() * Self::TICK;

        Self::try_from(round)
    }

    /// Clamps a value to the valid price range and rounds to the nearest tick.
    ///
    /// This method guarantees a valid [`Price`] by clamping the input to the valid range before
    /// rounding.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::Price;
    ///
    /// // Value within range
    /// let price = Price::clamp_from(100_000.0);
    /// assert_eq!(price.into_f64(), 100_000.0);
    ///
    /// // Value above maximum is clamped
    /// let price = Price::clamp_from(200_000_000.0);
    /// assert_eq!(price.into_f64(), 100_000_000.0);
    ///
    /// // Value below minimum is clamped
    /// let price = Price::clamp_from(0.1);
    /// assert_eq!(price.into_f64(), 1.0);
    /// ```
    pub fn clamp_from(value: f64) -> Self {
        let value = value.clamp(Self::MIN.0, Self::MAX.0);

        Self::round(value).expect("clamped `value` must be within valid range")
    }

    /// Applies a discount percentage to the current price.
    ///
    /// Calculates a new price reduced by the specified percentage and rounds to the nearest valid
    /// tick size.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::{Price, BoundedPercentage};
    ///
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let discount = BoundedPercentage::try_from(10.0).unwrap(); // 10% discount
    ///
    /// let discounted_price = price.apply_discount(discount).unwrap();
    /// assert_eq!(discounted_price.into_f64(), 90_000.0);
    /// ```
    pub fn apply_discount(
        &self,
        percentage: BoundedPercentage,
    ) -> Result<Self, PriceValidationError> {
        let target_price = self.0 - self.0 * percentage.into_f64() / 100.0;

        Self::round(target_price)
    }

    /// Applies a gain percentage to the current price.
    ///
    /// Calculates a new price increased by the specified percentage and rounds to the nearest valid
    /// tick size.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v2::models::{Price, LowerBoundedPercentage};
    ///
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let gain = LowerBoundedPercentage::try_from(20.0).unwrap(); // 20% gain
    ///
    /// let increased_price = price.apply_gain(gain).unwrap();
    /// assert_eq!(increased_price.into_f64(), 120_000.0);
    /// ```
    pub fn apply_gain(
        &self,
        percentage: LowerBoundedPercentage,
    ) -> Result<Self, PriceValidationError> {
        let target_price = self.0 + self.0 * percentage.into_f64() / 100.0;

        Self::round(target_price)
    }
}

impl From<Price> for f64 {
    fn from(value: Price) -> f64 {
        value.0
    }
}

impl TryFrom<f64> for Price {
    type Error = PriceValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(PriceValidationError::TooLow { value });
        }

        if value > Self::MAX.0 {
            return Err(PriceValidationError::TooHigh { value });
        }

        let calc = value / Self::TICK;
        if (calc.round() - calc).abs() > 1e-10 {
            return Err(PriceValidationError::NotMultipleOfTick { value });
        }

        Ok(Price(value))
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
        self.0.fmt(f)
    }
}

impl Serialize for Price {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_util::float_without_decimal::serialize(&self.0, serializer)
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
