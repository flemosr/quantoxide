use std::{convert::TryFrom, fmt, num::NonZeroU64, ops::Add};

use serde::{Deserialize, Serialize, de};

use super::{
    SATS_PER_BTC,
    error::{MarginValidationError, TradeValidationError},
    leverage::Leverage,
    price::Price,
    quantity::Quantity,
    trade::TradeSide,
};

/// A validated margin value denominated in satoshis.
///
/// Margin represents the collateral required to open a leveraged trading position.
/// This type ensures that margin values meet minimum requirements and can be safely used with
/// [`Trade`] implementations.
///
/// Margin values must be integers greater than or equal to [`Margin::MIN`] (1 satoshi).
///
/// # Examples
///
/// ```
/// use lnm_sdk::models::Margin;
///
/// // Create a margin value from satoshis
/// let margin = Margin::try_from(10_000).unwrap();
/// assert_eq!(margin.into_u64(), 10_000);
///
/// // Values below the minimum will fail
/// assert!(Margin::try_from(0).is_err());
/// ```
///
/// [`Trade`]: crate::models::Trade
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Margin(u64);

impl Margin {
    /// The minimum allowed margin value (1 satoshi).
    pub const MIN: Self = Self(1);

    /// Converts the margin value to its underlying `u64` representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::Margin;
    ///
    /// let margin = Margin::try_from(10_000).unwrap();
    /// assert_eq!(margin.into_u64(), 10_000);
    /// ```
    pub fn into_u64(self) -> u64 {
        self.into()
    }

    /// Converts the margin value to `i64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::Margin;
    ///
    /// let margin = Margin::try_from(10_000).unwrap();
    /// assert_eq!(margin.into_i64(), 10_000);
    /// ```
    pub fn into_i64(self) -> i64 {
        self.into()
    }

    /// Converts the margin value to `f64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::Margin;
    ///
    /// let margin = Margin::try_from(10_000).unwrap();
    /// assert_eq!(margin.into_f64(), 10_000.0);
    /// ```
    pub fn into_f64(self) -> f64 {
        self.into()
    }

    /// Calculates margin from quantity (USD), price (BTC/USD), and leverage.
    ///
    /// The margin is calculated using the formula:
    ///
    /// margin = (quantity * SATS_PER_BTC) / (price * leverage)
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::{Margin, Quantity, Price, Leverage};
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// let price = Price::try_from(100_000.0).unwrap();
    /// let leverage = Leverage::try_from(10.0).unwrap();
    ///
    /// let margin = Margin::calculate(quantity, price, leverage);
    /// ```
    pub fn calculate(quantity: Quantity, price: Price, leverage: Leverage) -> Self {
        let margin =
            quantity.into_f64() * (SATS_PER_BTC / (price.into_f64() * leverage.into_f64()));

        Self::try_from(margin.ceil() as u64).expect("must result in valid `Margin`")
    }

    /// Estimates margin from a target liquidation price.
    ///
    /// Calculates the required margin to achieve a specific liquidation price for a position
    /// with the given quantity and entry price.
    ///
    /// + For long positions (Buy): liquidation price must be below entry price
    /// + For short positions (Sell): liquidation price must be above entry price
    ///
    /// # Examples
    ///
    /// ```
    /// use lnm_sdk::models::{Margin, Quantity, Price, TradeSide};
    ///
    /// let quantity = Quantity::try_from(1_000).unwrap();
    /// let entry_price = Price::try_from(100_000.0).unwrap();
    /// let liquidation_price = Price::try_from(95_000.0).unwrap();
    ///
    /// let margin = Margin::est_from_liquidation_price(
    ///     TradeSide::Buy,
    ///     quantity,
    ///     entry_price,
    ///     liquidation_price
    /// ).unwrap();
    /// ```
    pub fn est_from_liquidation_price(
        side: TradeSide,
        quantity: Quantity,
        price: Price,
        liquidation: Price,
    ) -> Result<Self, TradeValidationError> {
        match side {
            TradeSide::Buy if liquidation >= price => {
                return Err(TradeValidationError::LiquidationNotBelowPriceForLong {
                    liquidation,
                    price,
                });
            }
            TradeSide::Sell if liquidation <= price => {
                return Err(TradeValidationError::LiquidationNotAbovePriceForShort {
                    liquidation,
                    price,
                });
            }
            _ => {}
        }

        // Calculate 'a' and 'b' from `trade_utils::estimate_liquidation_price`

        let a = 1.0 / price.into_f64();

        let b = match side {
            TradeSide::Buy => {
                // liquidation_price = 1.0 / (a + b)
                1.0 / liquidation.into_f64() - a
            }
            TradeSide::Sell => {
                // liquidation_price = 1.0 / (a - b)
                a - 1.0 / liquidation.into_f64()
            }
        };

        assert!(b > 0.0, "'b' must be positive from validations above");

        let floored_margin = b * SATS_PER_BTC * quantity.into_f64();

        let margin =
            Margin::try_from(floored_margin.ceil() as u64).expect("must be valid `Margin`");

        Ok(margin)
    }
}

impl Add for Margin {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Margin(self.0 + other.0)
    }
}

impl From<Margin> for u64 {
    fn from(value: Margin) -> Self {
        value.0
    }
}

impl From<Margin> for i64 {
    fn from(value: Margin) -> Self {
        value.0 as i64
    }
}

impl From<Margin> for f64 {
    fn from(value: Margin) -> Self {
        value.0 as f64
    }
}

impl From<NonZeroU64> for Margin {
    fn from(value: NonZeroU64) -> Self {
        Margin(value.get())
    }
}

impl TryFrom<u64> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value < Self::MIN.0 {
            return Err(MarginValidationError::TooLow);
        }

        Ok(Self(value as u64))
    }
}

impl TryFrom<i32> for Margin {
    type Error = MarginValidationError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value.max(0) as u64)
    }
}

impl fmt::Display for Margin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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

#[cfg(test)]
mod tests {
    use super::super::trade::util as trade_util;

    use super::*;

    #[test]
    fn test_calculate_margin() {
        let quantity = Quantity::try_from(5).unwrap();
        let price = Price::try_from(95000).unwrap();
        let leverage = Leverage::try_from(1.0).unwrap();

        let margin = Margin::calculate(quantity, price, leverage);
        assert_eq!(margin.into_u64(), 5264);

        let leverage = Leverage::try_from(2.0).unwrap();
        let margin = Margin::calculate(quantity, price, leverage);
        assert_eq!(margin.into_u64(), 2632);

        let leverage = Leverage::try_from(50.0).unwrap();
        let margin = Margin::calculate(quantity, price, leverage);
        assert_eq!(margin.into_u64(), 106);

        let leverage = Leverage::try_from(100.0).unwrap();
        let margin = Margin::calculate(quantity, price, leverage);
        assert_eq!(margin.into_u64(), 53);

        // Edge case: Min margin
        let margin = Margin::calculate(Quantity::MIN, Price::MAX, Leverage::MAX);
        assert_eq!(margin, Margin::MIN);

        // Edge case: Max reachable margin
        let margin = Margin::calculate(Quantity::MAX, Price::MIN, Leverage::MIN);
        assert_eq!(margin.into_u64(), 50_000_000_000_000);
    }

    #[test]
    fn test_margin_from_liquidation_price_calculation() {
        // Test case 1: Buy side with low leverage

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1_000).unwrap();
        let entry_price = Price::try_from(100_000).unwrap();
        let leverage = Leverage::MIN;

        let liquidation_price =
            trade_util::estimate_liquidation_price(side, quantity, entry_price, leverage);
        let margin =
            Margin::est_from_liquidation_price(side, quantity, entry_price, liquidation_price)
                .expect("should calculate valid margin");
        let expected_margin = Margin::calculate(quantity, entry_price, leverage);

        assert!(
            (margin.into_i64() - expected_margin.into_i64()).abs() <= 999,
            "Margin difference too large: calculated {} vs expected {}",
            margin.into_u64(),
            expected_margin.into_u64()
        );

        // Test case 2: Buy side with high leverage

        let leverage = Leverage::MAX;
        let liquidation_price =
            trade_util::estimate_liquidation_price(side, quantity, entry_price, leverage);
        let margin =
            Margin::est_from_liquidation_price(side, quantity, entry_price, liquidation_price)
                .expect("should calculate valid margin");
        let expected_margin = Margin::calculate(quantity, entry_price, leverage);

        assert!(
            (margin.into_i64() - expected_margin.into_i64()).abs() <= 999,
            "Margin difference too large: calculated {} vs expected {}",
            margin.into_u64(),
            expected_margin.into_u64()
        );

        // Test case 3: Sell side with low leverage

        let side = TradeSide::Sell;
        let leverage = Leverage::MIN;
        let liquidation_price =
            trade_util::estimate_liquidation_price(side, quantity, entry_price, leverage);
        let margin =
            Margin::est_from_liquidation_price(side, quantity, entry_price, liquidation_price)
                .expect("should calculate valid margin");
        let expected_margin = Margin::calculate(quantity, entry_price, leverage);

        assert!(
            (margin.into_i64() - expected_margin.into_i64()).abs() <= 999,
            "Margin difference too large: calculated {} vs expected {}",
            margin.into_u64(),
            expected_margin.into_u64()
        );
    }
}
