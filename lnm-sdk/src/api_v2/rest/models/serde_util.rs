pub(in crate::api_v2) mod trade_side {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::shared::models::trade::TradeSide;

    pub fn serialize<S>(value: &TradeSide, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match value {
            TradeSide::Buy => "b",
            TradeSide::Sell => "s",
        })
    }

    // FIXME: As of Nov 11 2025, the LNMarkets API returns "buy" / "sell" when fetching recently
    // opened trades via API v2 . It returned only "b" / "s" until recently. Not clear if this
    // behavior is temporary.
    // Handling all cases for now.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<TradeSide, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "b" | "buy" => Ok(TradeSide::Buy),
            "s" | "sell" => Ok(TradeSide::Sell),
            _ => Err(serde::de::Error::custom(
                format!("unknown trade side: {s}",),
            )),
        }
    }
}
