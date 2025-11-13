pub(crate) mod float_without_decimal {
    use serde::Serializer;

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if value.fract() == 0.0 {
            serializer.serialize_i64(*value as i64)
        } else {
            serializer.serialize_f64(*value)
        }
    }
}

pub(crate) mod price_option {
    use serde::{Deserialize, de};

    use crate::shared::models::price::Price;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Price>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let opt_price_f64 = Option::<f64>::deserialize(deserializer)?;

        match opt_price_f64 {
            None => Ok(None),
            Some(price_f64) => {
                if price_f64 == 0.0 {
                    Ok(None)
                } else {
                    match Price::try_from(price_f64) {
                        Ok(price) => Ok(Some(price)),
                        Err(e) => Err(de::Error::custom(e.to_string())),
                    }
                }
            }
        }
    }
}
