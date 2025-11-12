use std::{collections::HashSet, fmt};

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

use super::{
    error::{ConnectionResult, Result, WebSocketApiError, WebSocketConnectionError},
    state::WsConnectionStatus,
};

#[derive(Serialize, Debug, PartialEq, Eq)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    id: String,
    params: Vec<String>,
}

impl JsonRpcRequest {
    pub fn try_to_bytes(&self) -> ConnectionResult<Vec<u8>> {
        let request_json =
            serde_json::to_string(&self).map_err(WebSocketConnectionError::EncodeJson)?;
        let bytes = request_json.into_bytes();
        Ok(bytes)
    }
}

#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum WebSocketChannel {
    FuturesBtcUsdIndex,
    FuturesBtcUsdLastPrice,
}

impl WebSocketChannel {
    fn as_str(&self) -> &'static str {
        match self {
            WebSocketChannel::FuturesBtcUsdIndex => "futures:btc_usd:index",
            WebSocketChannel::FuturesBtcUsdLastPrice => "futures:btc_usd:last-price",
        }
    }
}

impl TryFrom<&str> for WebSocketChannel {
    type Error = WebSocketApiError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "futures:btc_usd:index" => Ok(WebSocketChannel::FuturesBtcUsdIndex),
            "futures:btc_usd:last-price" => Ok(WebSocketChannel::FuturesBtcUsdLastPrice),
            _ => Err(WebSocketApiError::UnknownChannel(value.to_string())),
        }
    }
}

impl fmt::Display for WebSocketChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LnmJsonRpcReqMethod {
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

/// Added to pub errors for context
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LnmJsonRpcRequest {
    method: LnmJsonRpcReqMethod,
    id: String,
    channels: Vec<WebSocketChannel>,
}

impl LnmJsonRpcRequest {
    pub(super) fn new(method: LnmJsonRpcReqMethod, channels: Vec<WebSocketChannel>) -> Self {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill(&mut random_bytes);
        let id = hex::encode(random_bytes);

        Self {
            method,
            id,
            channels,
        }
    }

    pub(super) fn id(&self) -> &String {
        &self.id
    }

    pub(super) fn channels(&self) -> &Vec<WebSocketChannel> {
        &self.channels
    }

    pub(super) fn check_confirmation(&self, id: &String, channels: &[WebSocketChannel]) -> bool {
        if self.id() != id {
            return false;
        }

        let set_a: HashSet<&WebSocketChannel> = self.channels().iter().collect();
        let set_b: HashSet<&WebSocketChannel> = channels.iter().collect();
        set_a == set_b
    }

    pub(super) fn try_into_bytes(self) -> ConnectionResult<Vec<u8>> {
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

/// Added to pub errors for context
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<String>,
    method: Option<String>,
    result: Option<Value>,
    error: Option<Value>,
    params: Option<Value>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum LastTickDirection {
    MinusTick,
    ZeroMinusTick,
    PlusTick,
    ZeroPlusTick,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PriceTick {
    time: DateTime<Utc>,
    last_price: f64,
    // As of Nov 11 2025, some ticks may be received without the
    // `last_tick_direction` property when subscribing to
    // LNM's 'futures:btc_usd:last-price' channel.
    last_tick_direction: Option<LastTickDirection>,
}

impl PriceTick {
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn last_price(&self) -> f64 {
        self.last_price
    }

    pub fn last_tick_direction(&self) -> Option<LastTickDirection> {
        self.last_tick_direction
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PriceIndex {
    time: DateTime<Utc>,
    index: f64,
}

impl PriceIndex {
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    pub fn index(&self) -> f64 {
        self.index
    }
}

#[derive(Debug, Clone)]
pub(super) enum SubscriptionData {
    PriceTick(PriceTick),
    PriceIndex(PriceIndex),
}

#[derive(Clone, Debug)]
pub(super) enum LnmJsonRpcResponse {
    Confirmation {
        id: String,
        channels: Vec<WebSocketChannel>,
    },
    Subscription(SubscriptionData),
}

impl TryFrom<JsonRpcResponse> for LnmJsonRpcResponse {
    type Error = WebSocketConnectionError;

    fn try_from(response: JsonRpcResponse) -> ConnectionResult<Self> {
        if let Some(id) = &response.id {
            let try_parse_confirmation_data = || -> Option<(String, Vec<WebSocketChannel>)> {
                let result = response.result.as_ref()?;

                let result_array = result.as_array()?;

                let channels: Vec<WebSocketChannel> = result_array
                    .iter()
                    .filter_map(|channel| WebSocketChannel::try_from(channel.as_str()?).ok())
                    .collect();

                if channels.len() != result_array.len() {
                    return None;
                }

                Some((id.clone(), channels))
            };

            if let Some((id, channels)) = try_parse_confirmation_data() {
                return Ok(Self::Confirmation { id, channels });
            }

            return Err(WebSocketConnectionError::UnexpectedJsonRpcResponse(
                response,
            ));
        }

        if response.method.as_deref() == Some("subscription") {
            let try_parse_subscription_data = || -> Option<SubscriptionData> {
                let params = response.params.as_ref()?;

                let params_obj = params.as_object()?;

                let channel: WebSocketChannel = params_obj
                    .get("channel")
                    .and_then(|v| v.as_str())?
                    .try_into()
                    .ok()?;

                let data = params_obj.get("data")?.clone();

                let data = match channel {
                    WebSocketChannel::FuturesBtcUsdLastPrice => {
                        let price_tick: PriceTick = serde_json::from_value(data).ok()?;
                        SubscriptionData::PriceTick(price_tick)
                    }
                    WebSocketChannel::FuturesBtcUsdIndex => {
                        let price_index: PriceIndex = serde_json::from_value(data).ok()?;
                        SubscriptionData::PriceIndex(price_index)
                    }
                };

                Some(data)
            };

            if let Some(data) = try_parse_subscription_data() {
                return Ok(Self::Subscription(data));
            }

            return Err(WebSocketConnectionError::UnexpectedJsonRpcResponse(
                response,
            ));
        }

        Err(WebSocketConnectionError::UnexpectedJsonRpcResponse(
            response,
        ))
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

#[derive(Debug, Clone)]
pub enum WebSocketUpdate {
    PriceTick(PriceTick),
    PriceIndex(PriceIndex),
    ConnectionStatus(WsConnectionStatus),
}

impl From<WsConnectionStatus> for WebSocketUpdate {
    fn from(value: WsConnectionStatus) -> Self {
        Self::ConnectionStatus(value)
    }
}

impl From<SubscriptionData> for WebSocketUpdate {
    fn from(data: SubscriptionData) -> Self {
        match data {
            SubscriptionData::PriceTick(price_tick) => WebSocketUpdate::PriceTick(price_tick),
            SubscriptionData::PriceIndex(price_index) => WebSocketUpdate::PriceIndex(price_index),
        }
    }
}
