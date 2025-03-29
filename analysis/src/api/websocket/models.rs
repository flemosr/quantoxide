use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use super::error::{Result, WebSocketApiError};

pub enum LnmJsonRpcMethod {
    Subscribe,
    Unsubscribe,
}

impl LnmJsonRpcMethod {
    fn as_str(&self) -> &'static str {
        match self {
            LnmJsonRpcMethod::Subscribe => "v1/public/subscribe",
            LnmJsonRpcMethod::Unsubscribe => "v1/public/unsubscribe",
        }
    }
}

impl fmt::Display for LnmJsonRpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    pub id: String,
    params: Vec<String>,
}

impl JsonRpcRequest {
    pub fn new(method: LnmJsonRpcMethod, params: Vec<String>) -> Self {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill(&mut random_bytes);
        let request_id = hex::encode(random_bytes);

        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            id: request_id,
            params,
        }
    }

    pub fn try_to_bytes(&self) -> Result<Vec<u8>> {
        let request_json =
            serde_json::to_string(&self).map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let bytes = request_json.into_bytes();
        Ok(bytes)
    }
}

pub enum LnmWebSocketChannels {
    FuturesBtcUsdIndex,
    FuturesBtcUsdLastPrice,
}

impl LnmWebSocketChannels {
    fn as_str(&self) -> &'static str {
        match self {
            LnmWebSocketChannels::FuturesBtcUsdIndex => "futures:btc_usd:index",
            LnmWebSocketChannels::FuturesBtcUsdLastPrice => "futures:btc_usd:last-price",
        }
    }
}

impl TryFrom<&str> for LnmWebSocketChannels {
    type Error = WebSocketApiError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "futures:btc_usd:index" => Ok(LnmWebSocketChannels::FuturesBtcUsdIndex),
            "futures:btc_usd:last-price" => Ok(LnmWebSocketChannels::FuturesBtcUsdLastPrice),
            _ => Err(WebSocketApiError::Generic(format!(
                "Unknown channel: {value}",
            ))),
        }
    }
}

impl fmt::Display for LnmWebSocketChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    pub id: Option<String>,
    pub method: Option<String>,
    result: Option<Value>,
    error: Option<Value>,
    params: Option<Value>,
}

#[derive(Debug, Deserialize, Clone)]
enum LastTickDirection {
    Minus,
    ZeroMinus,
    Plus,
    ZeroPlus,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PriceTickLNM {
    time: DateTime<Utc>,
    last_price: f64,
    last_tick_direction: LastTickDirection,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PriceIndexLNM {
    index: f64,
    time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum WebSocketDataLNM {
    PriceTick(PriceTickLNM),
    PriceIndex(PriceIndexLNM),
}

impl TryFrom<JsonRpcResponse> for WebSocketDataLNM {
    type Error = WebSocketApiError;

    fn try_from(response: JsonRpcResponse) -> Result<Self> {
        if response.method.as_deref() != Some("subscription") {
            return Err(WebSocketApiError::Generic(
                "Not a subscription message".to_string(),
            ));
        }

        let params = response.params.ok_or_else(|| {
            WebSocketApiError::Generic("Missing params in subscription".to_string())
        })?;

        let params_obj = params
            .as_object()
            .ok_or_else(|| WebSocketApiError::Generic("Params is not an object".to_string()))?;

        let channel: LnmWebSocketChannels = params_obj
            .get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| WebSocketApiError::Generic("Missing channel in params".to_string()))?
            .try_into()?;

        let data = params_obj
            .get("data")
            .ok_or_else(|| WebSocketApiError::Generic("Missing data in params".to_string()))?;

        let data = match channel {
            LnmWebSocketChannels::FuturesBtcUsdLastPrice => {
                let price_tick: PriceTickLNM =
                    serde_json::from_value(data.clone()).map_err(|e| {
                        WebSocketApiError::Generic(format!("Failed to parse price tick: {}", e))
                    })?;
                WebSocketDataLNM::PriceTick(price_tick)
            }
            LnmWebSocketChannels::FuturesBtcUsdIndex => {
                let price_index: PriceIndexLNM =
                    serde_json::from_value(data.clone()).map_err(|e| {
                        WebSocketApiError::Generic(format!("Failed to parse price index: {}", e))
                    })?;
                WebSocketDataLNM::PriceIndex(price_index)
            }
        };

        Ok(data)
    }
}
