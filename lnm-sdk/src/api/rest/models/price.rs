use serde::{Deserialize, Serialize, de};
use std::{cmp::Ordering, convert::TryFrom};

use super::{error::PriceValidationError, utils};

#[derive(Debug, Clone, Copy)]
pub struct Price(f64);

impl Price {
    pub fn to_f64(&self) -> f64 {
        self.0
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

impl PartialEq for Price {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Price {}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        // Since we guarantee the values are finite, we can use partial_cmp and unwrap
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq<f64> for Price {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for Price {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<Price> for f64 {
    fn eq(&self, other: &Price) -> bool {
        *self == other.0
    }
}

impl PartialOrd<Price> for f64 {
    fn partial_cmp(&self, other: &Price) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialEq<i32> for Price {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<i32> for Price {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<Price> for i32 {
    fn eq(&self, other: &Price) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<Price> for i32 {
    fn partial_cmp(&self, other: &Price) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
    }
}

impl PartialEq<u32> for Price {
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other as f64
    }
}

impl PartialOrd<u32> for Price {
    fn partial_cmp(&self, other: &u32) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as f64))
    }
}

impl PartialEq<Price> for u32 {
    fn eq(&self, other: &Price) -> bool {
        *self as f64 == other.0
    }
}

impl PartialOrd<Price> for u32 {
    fn partial_cmp(&self, other: &Price) -> Option<Ordering> {
        (*self as f64).partial_cmp(&other.0)
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
