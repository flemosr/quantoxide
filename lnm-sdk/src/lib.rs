#![doc = include_str!("../README.md")]

/// API v2 implementation.
///
/// Contains all types, clients, and functionality necessary to work with API v2 including REST and
/// WebSocket clients, models, and error types.
///
/// # Example
///
/// ```rust
/// use lnm_sdk::api_v2::{
///     RestClient, RestClientConfig, WebSocketChannel, WebSocketClient, WebSocketClientConfig,
///     WebSocketUpdate, error::*, models::*,
/// };
/// ```
pub mod api_v2;

/// API v3 implementation.
///
/// Contains all types, clients, and functionality necessary to work with API v3, including REST
/// client, models, and error types.
///
/// # Example
///
/// ```rust
/// use lnm_sdk::api_v3::{RestClient, RestClientConfig, models::*, error::*};
/// ```
pub mod api_v3;

mod shared;

mod sealed {
    pub trait Sealed {}
}
