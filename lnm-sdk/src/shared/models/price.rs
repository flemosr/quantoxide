use std::{cmp::Ordering, convert::TryFrom, fmt};

use serde::{Deserialize, Serialize, de};

use super::{
    error::{PercentageCappedValidationError, PercentageValidationError, PriceValidationError},
    serde_util,
};

/// A validated decimal percentage value constrained within a specific range.
///
/// Percentage values must be:
/// + Greater than or equal to [`PercentageCapped::MIN`] (0.0%)
/// + Less than or equal to [`PercentageCapped::MAX`] (100.0%)
///
/// This bounded range makes it suitable for percentage calculations where both minimum and maximum
/// limits are required, such as position distributions and discount calculations.
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v3::models::PercentageCapped;
///
/// // Create a bounded decimal percentage value
/// let percentage = PercentageCapped::try_from(50.0).unwrap(); // 50%
/// assert_eq!(percentage.as_f64(), 50.0);
///
/// // Values outside the valid range will fail
/// assert!(PercentageCapped::try_from(-0.01).is_err());
/// assert!(PercentageCapped::try_from(100.1).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PercentageCapped(f64);

impl PercentageCapped {
    /// The minimum allowed percentage value (0.0%).
    pub const MIN: Self = Self(0.);

    /// The maximum allowed percentage value (100.0%).
    pub const MAX: Self = Self(100.);

    /// Returns the percentage value as its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::PercentageCapped;
    ///
    /// let percentage = PercentageCapped::try_from(25.5).unwrap();
    /// assert_eq!(percentage.as_f64(), 25.5);
    /// ```
    pub fn as_f64(&self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for PercentageCapped {
    type Error = PercentageCappedValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(PercentageCappedValidationError::BelowMinimum { value });
        }

        if value > Self::MAX.0 {
            return Err(PercentageCappedValidationError::AboveMaximum { value });
        }

        Ok(Self(value))
    }
}

impl TryFrom<i32> for PercentageCapped {
    type Error = PercentageCappedValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl From<PercentageCapped> for f64 {
    fn from(value: PercentageCapped) -> f64 {
        value.0
    }
}

impl Eq for PercentageCapped {}

impl Ord for PercentageCapped {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("`PercentageCapped` must be finite")
    }
}

impl PartialOrd for PercentageCapped {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for PercentageCapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A validated decimal percentage value constrained only by a lower bound.
///
/// Percentage values must be:
/// + Greater than or equal to [`Percentage::MIN`] (0.0%)
/// + Finite (not infinity)
///
/// This type is suitable for percentage calculations where only a minimum threshold is needed, with
/// no practical upper limit other than it must be a finite value, such as gain calculations.
///
/// # Examples
///
/// ```
/// use lnm_sdk::api_v3::models::Percentage;
///
/// // Create a lower-bounded percentage value
/// let percentage = Percentage::try_from(150.0).unwrap();
/// assert_eq!(percentage.as_f64(), 150.0);
///
/// // Values below the minimum will fail
/// assert!(Percentage::try_from(-0.01).is_err());
///
/// // Non-finite values will fail
/// assert!(Percentage::try_from(f64::INFINITY).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percentage(f64);

impl Percentage {
    /// The minimum allowed percentage value (0.0%).
    pub const MIN: Self = Self(0.);

    /// Returns the percentage value as its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Percentage;
    ///
    /// let percentage = Percentage::try_from(200.0).unwrap();
    /// assert_eq!(percentage.as_f64(), 200.0);
    /// ```
    pub fn as_f64(&self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Percentage {
    type Error = PercentageValidationError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(PercentageValidationError::BelowMinimum { value });
        }

        if !value.is_finite() {
            return Err(PercentageValidationError::NotFinite);
        }

        Ok(Self(value))
    }
}

impl TryFrom<i32> for Percentage {
    type Error = PercentageValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl From<Percentage> for f64 {
    fn from(value: Percentage) -> f64 {
        value.0
    }
}

impl Eq for Percentage {}

impl Ord for Percentage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("`Percentage` must be finite")
    }
}

impl PartialOrd for Percentage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<PercentageCapped> for Percentage {
    fn from(value: PercentageCapped) -> Self {
        Self(value.0)
    }
}

impl fmt::Display for Percentage {
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
/// use lnm_sdk::api_v3::models::Price;
///
/// // Create a price value from USD amount
/// let price = Price::try_from(100_000.0).unwrap();
/// assert_eq!(price.as_f64(), 100_000.0);
///
/// // Values outside the valid range will fail
/// assert!(Price::try_from(0.5).is_err());
/// assert!(Price::try_from(150_000_000.0).is_err());
///
/// // Values not aligned to the tick size will fail
/// assert!(Price::try_from(100_000.25).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
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

    /// Rounds a value down to the nearest valid price.
    ///
    /// The value is rounded down to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Price;
    ///
    /// let price = Price::round_down(100_000.8).unwrap();
    /// assert_eq!(price.as_f64(), 100_000.5);
    /// ```
    pub fn round_down<T>(value: T) -> Result<Self, PriceValidationError>
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let rounded_down = (as_f64 / Self::TICK).floor() * Self::TICK;

        Self::try_from(rounded_down)
    }

    /// Rounds a value up to the nearest valid price.
    ///
    /// The value is rounded up to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Price;
    ///
    /// let price = Price::round_up(100_000.2).unwrap();
    /// assert_eq!(price.as_f64(), 100_000.5);
    /// ```
    pub fn round_up<T>(value: T) -> Result<Self, PriceValidationError>
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let rounded_up = (as_f64 / Self::TICK).ceil() * Self::TICK;

        Self::try_from(rounded_up)
    }

    /// Rounds a value to the nearest valid price.
    ///
    /// The value is rounded to the nearest multiple of [`Price::TICK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Price;
    ///
    /// let price = Price::round(100_000.6).unwrap();
    /// assert_eq!(price.as_f64(), 100_000.5);
    ///
    /// let price = Price::round(100_000.8).unwrap();
    /// assert_eq!(price.as_f64(), 100_001.0);
    /// ```
    pub fn round<T>(value: T) -> Result<Self, PriceValidationError>
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let rounded = (as_f64 / Self::TICK).round() * Self::TICK;

        Self::try_from(rounded)
    }

    /// Creates a `Price` by rounding and bounding the given value to the valid range.
    ///
    /// This method rounds the input to the nearest valid tick size and bounds it to the range
    /// ([Price::MIN], [Price::MAX]).
    /// It should be used to ensure a valid `Price` without error handling.
    ///
    /// **Note:** In order to validate whether a value is a valid price and receive an error for
    /// invalid values, use [`Price::try_from`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Price;
    ///
    /// // Value within range
    /// let price = Price::bounded(100_000.0);
    /// assert_eq!(price.as_f64(), 100_000.0);
    ///
    /// // Value above maximum is bounded to MAX
    /// let price = Price::bounded(200_000_000.0);
    /// assert_eq!(price.as_f64(), 100_000_000.0);
    ///
    /// // Value below minimum is bounded to MIN
    /// let price = Price::bounded(0.1);
    /// assert_eq!(price.as_f64(), 1.0);
    /// ```
    pub fn bounded<T>(value: T) -> Self
    where
        T: Into<f64>,
    {
        let as_f64: f64 = value.into();
        let value = as_f64.clamp(Self::MIN.0, Self::MAX.0);
        let rounded = (value / Self::TICK).round() * Self::TICK;

        Self(rounded)
    }

    /// Returns the price value as its underlying `f64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::Price;
    ///
    /// let price = Price::try_from(50_000.0).unwrap();
    /// assert_eq!(price.as_f64(), 50_000.0);
    /// ```
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// Applies a discount percentage to the current price.
    ///
    /// Calculates a new price reduced by the specified percentage and rounds to the nearest valid
    /// tick size.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::api_v3::models::{Price, PercentageCapped};
    ///
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let discount = PercentageCapped::try_from(10.0).unwrap(); // 10% discount
    ///
    /// let discounted_price = price.apply_discount(discount).unwrap();
    /// assert_eq!(discounted_price.as_f64(), 90_000.0);
    /// ```
    pub fn apply_discount(
        &self,
        percentage: PercentageCapped,
    ) -> Result<Self, PriceValidationError> {
        let target_price = self.0 - self.0 * percentage.as_f64() / 100.0;

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
    /// use lnm_sdk::api_v3::models::{Price, Percentage};
    ///
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let gain = Percentage::try_from(20.0).unwrap(); // 20% gain
    ///
    /// let increased_price = price.apply_gain(gain).unwrap();
    /// assert_eq!(increased_price.as_f64(), 120_000.0);
    /// ```
    pub fn apply_gain(&self, percentage: Percentage) -> Result<Self, PriceValidationError> {
        let target_price = self.0 + self.0 * percentage.as_f64() / 100.0;

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

impl TryFrom<u64> for Price {
    type Error = PriceValidationError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl TryFrom<i32> for Price {
    type Error = PriceValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as f64)
    }
}

impl Eq for Price {}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("`Price` must be finite")
    }
}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
