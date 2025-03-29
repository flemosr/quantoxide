use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use super::{
    error::{Result, WebSocketApiError},
    ConnectionState,
};

pub enum LnmJsonRpcReqMethod {
    Subscribe,
    Unsubscribe,
}

impl LnmJsonRpcReqMethod {
    fn as_str(&self) -> &'static str {
        match self {
            LnmJsonRpcReqMethod::Subscribe => "v1/public/subscribe",
            LnmJsonRpcReqMethod::Unsubscribe => "v1/public/unsubscribe",
        }
    }
}

impl fmt::Display for LnmJsonRpcReqMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct LnmJsonRpcRequest {
    jsonrpc: String,
    method: String,
    id: String,
    params: Vec<String>,
}

impl LnmJsonRpcRequest {
    pub fn new(method: LnmJsonRpcReqMethod, channels: Vec<LnmWebSocketChannel>) -> Self {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill(&mut random_bytes);
        let id = hex::encode(random_bytes);

        let channels = channels
            .into_iter()
            .map(|channel| channel.to_string())
            .collect();

        Self {
            jsonrpc: "2.0".to_string(),
            method: method.as_str().to_string(),
            id,
            params: channels,
        }
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn try_to_bytes(&self) -> Result<Vec<u8>> {
        let request_json =
            serde_json::to_string(&self).map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let bytes = request_json.into_bytes();
        Ok(bytes)
    }
}

#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum LnmWebSocketChannel {
    FuturesBtcUsdIndex,
    FuturesBtcUsdLastPrice,
}

impl LnmWebSocketChannel {
    fn as_str(&self) -> &'static str {
        match self {
            LnmWebSocketChannel::FuturesBtcUsdIndex => "futures:btc_usd:index",
            LnmWebSocketChannel::FuturesBtcUsdLastPrice => "futures:btc_usd:last-price",
        }
    }
}

impl TryFrom<&str> for LnmWebSocketChannel {
    type Error = WebSocketApiError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "futures:btc_usd:index" => Ok(LnmWebSocketChannel::FuturesBtcUsdIndex),
            "futures:btc_usd:last-price" => Ok(LnmWebSocketChannel::FuturesBtcUsdLastPrice),
            _ => Err(WebSocketApiError::Generic(format!(
                "Unknown channel: {value}",
            ))),
        }
    }
}

impl fmt::Display for LnmWebSocketChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    pub id: Option<String>,
    pub method: Option<String>,
    result: Option<Value>,
    error: Option<Value>,
    params: Option<Value>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(clippy::enum_variant_names)]
enum LastTickDirection {
    MinusTick,
    ZeroMinusTick,
    PlusTick,
    ZeroPlusTick,
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
pub enum WebSocketApiRes {
    PriceTick(PriceTickLNM),
    PriceIndex(PriceIndexLNM),
    ConnectionUpdate(ConnectionState),
}

impl From<&ConnectionState> for WebSocketApiRes {
    fn from(value: &ConnectionState) -> Self {
        Self::ConnectionUpdate(value.clone())
    }
}

impl TryFrom<JsonRpcResponse> for WebSocketApiRes {
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

        let channel: LnmWebSocketChannel = params_obj
            .get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| WebSocketApiError::Generic("Missing channel in params".to_string()))?
            .try_into()?;

        let data = params_obj
            .get("data")
            .ok_or_else(|| WebSocketApiError::Generic("Missing data in params".to_string()))?;

        let data = match channel {
            LnmWebSocketChannel::FuturesBtcUsdLastPrice => {
                let price_tick: PriceTickLNM =
                    serde_json::from_value(data.clone()).map_err(|e| {
                        WebSocketApiError::Generic(format!("Failed to parse price tick: {}", e))
                    })?;
                WebSocketApiRes::PriceTick(price_tick)
            }
            LnmWebSocketChannel::FuturesBtcUsdIndex => {
                let price_index: PriceIndexLNM =
                    serde_json::from_value(data.clone()).map_err(|e| {
                        WebSocketApiError::Generic(format!("Failed to parse price index: {}", e))
                    })?;
                WebSocketApiRes::PriceIndex(price_index)
            }
        };

        Ok(data)
    }
}
