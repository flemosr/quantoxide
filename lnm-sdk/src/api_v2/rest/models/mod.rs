/// Number of satoshis (sats) in a Bitcoin: 100_000_000
pub const SATS_PER_BTC: f64 = 100_000_000.;

pub(in crate::api_v2) mod error;
pub(in crate::api_v2) mod margin;
pub(in crate::api_v2) mod price;
pub(in crate::api_v2) mod price_history;
pub(in crate::api_v2) mod serde_util;
pub(in crate::api_v2) mod ticker;
pub(in crate::api_v2) mod trade;
pub(in crate::api_v2) mod user;
