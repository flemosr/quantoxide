use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use crate::api::error::{ApiError, Result};

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
            serde_json::to_string(&self).map_err(|e| ApiError::WebSocketGeneric(e.to_string()))?;
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
