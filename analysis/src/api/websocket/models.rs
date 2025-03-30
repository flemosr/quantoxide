use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt;

use super::{
    error::{Result, WebSocketApiError},
    ConnectionState,
};

#[derive(Serialize, Debug, PartialEq, Eq)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    id: String,
    params: Vec<String>,
}

impl JsonRpcRequest {
    pub fn try_to_bytes(&self) -> Result<Vec<u8>> {
        let request_json =
            serde_json::to_string(&self).map_err(|e| WebSocketApiError::Generic(e.to_string()))?;
        let bytes = request_json.into_bytes();
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LnmJsonRpcRequest {
    method: LnmJsonRpcReqMethod,
    id: String,
    channels: Vec<LnmWebSocketChannel>,
}

impl LnmJsonRpcRequest {
    pub fn new(method: LnmJsonRpcReqMethod, channels: Vec<LnmWebSocketChannel>) -> Self {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill(&mut random_bytes);
        let id = hex::encode(random_bytes);

        Self {
            method,
            id,
            channels,
        }
    }

    pub fn id(&self) -> &String {
        &self.id
    }

    pub fn channels(&self) -> &Vec<LnmWebSocketChannel> {
        &self.channels
    }

    pub fn try_into_bytes(self) -> Result<Vec<u8>> {
        JsonRpcRequest::from(self).try_to_bytes()
    }
}

impl From<LnmJsonRpcRequest> for JsonRpcRequest {
    fn from(request: LnmJsonRpcRequest) -> Self {
        let channels = request
            .channels
            .into_iter()
            .map(|channel| channel.to_string())
            .collect();

        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: request.method.as_str().to_string(),
            id: request.id,
            params: channels,
        }
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
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<String>,
    method: Option<String>,
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

#[derive(Clone, Debug)]
pub enum LnmJsonRpcResponse {
    Confirmation {
        id: String,
        channels: Vec<LnmWebSocketChannel>,
    },
    Subscription(WebSocketApiRes),
}

impl TryFrom<JsonRpcResponse> for LnmJsonRpcResponse {
    type Error = WebSocketApiError;

    fn try_from(response: JsonRpcResponse) -> Result<Self> {
        if let Some(id) = response.id {
            let result = response.result.ok_or_else(|| {
                WebSocketApiError::Generic("Missing result in confirmation".to_string())
            })?;

            let result_array = result.as_array().ok_or_else(|| {
                WebSocketApiError::Generic("Result is not an array in confirmation".to_string())
            })?;

            let result_array_len = result_array.len();
            let channels: Vec<LnmWebSocketChannel> = result_array
                .into_iter()
                .filter_map(|channel| LnmWebSocketChannel::try_from(channel.as_str()?).ok())
                .collect();

            if channels.len() != result_array_len {
                return Err(WebSocketApiError::Generic(
                    "Received unknown channel in result array".to_string(),
                ));
            }

            return Ok(Self::Confirmation { id, channels });
        }

        if response.method.as_deref() == Some("subscription") {
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
                            WebSocketApiError::Generic(format!(
                                "Failed to parse price index: {}",
                                e
                            ))
                        })?;
                    WebSocketApiRes::PriceIndex(price_index)
                }
            };

            return Ok(Self::Subscription(data));
        }

        return Err(WebSocketApiError::Generic(
            "Unknown JSON RPC response".to_string(),
        ));
    }
}

impl<'de> Deserialize<'de> for LnmJsonRpcResponse {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let json_rpc_response = JsonRpcResponse::deserialize(deserializer)?;

        LnmJsonRpcResponse::try_from(json_rpc_response)
            .map_err(|e| de::Error::custom(format!("Conversion error: {}", e)))
    }
}
