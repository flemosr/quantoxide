# LN Markets SDK

A Rust SDK for interacting with [LN Markets]. Supports REST API v3, REST API v2, and WebSocket API.

> **Note:** This is an unofficial SDK. API v3 support is functional but not yet feature-complete. 
> For implementation status, see [docs/api-v3-implementation.md](docs/api-v3-implementation.md).

## Getting Started

### Rust Version

This project's MSRV is `1.88`.

### Dependencies

```toml
[dependencies]
lnm-sdk = "<lnm-sdk-version>"
```

## Usage

This SDK provides strong type-safety with validated types for all parameters used in trade 
operations. All necessary models can be imported via the `models` mod of the API version in question.

```rust
// When working with API v3
use lnm_sdk::api_v3::{RestClient, RestClientConfig, models::*, error::*};

// When working with API v2
use lnm_sdk::api_v2::{
    RestClient, RestClientConfig, WebSocketChannel, WebSocketClient, WebSocketClientConfig,
    WebSocketUpdate, error::*, models::*,
};
```

## Examples

### REST API v3 - Public

```rust
use lnm_sdk::api_v3::{RestClient, RestClientConfig};

//...

let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

let rest = RestClient::new(RestClientConfig::default(), &domain)?;
    
// Get the futures ticker
let _ticker = rest.futures_data.get_ticker().await?;

// Get candles (OHLCs) history
let _candles = rest
    .futures_data
    .get_candles(None, None, None, None, None)
    .await?;
```

For more complete public API examples, see
[`lnm-sdk/examples/v3_rest_public.rs`](examples/v3_rest_public.rs).

### REST API v3 - Authenticated

```rust
use lnm_sdk::api_v3::{
    RestClient, RestClientConfig,
    models::{Leverage, Quantity, TradeExecution, TradeSide, TradeSize},
};

// ...

let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");
let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY must be set");
let secret = env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET must be set");
let passphrase = env::var("LNM_API_V3_PASSPHRASE").expect("LNM_API_V3_PASSPHRASE must be set");

let rest = RestClient::with_credentials(
    RestClientConfig::default(),
    &domain,
    key,
    secret,
    passphrase,
)?;
    
// Get account information
let _account = rest.account.get_account().await?;

// Place a new isolated trade
let trade = rest
    .futures_isolated
    .new_trade(
        TradeSide::Buy,
        TradeSize::from(Quantity::try_from(1)?), // 1 USD
        Leverage::try_from(30)?,                 // 30x leverage
        TradeExecution::Market,
        None, // stoploss
        None, // takeprofit
        None, // client trade id
    )
    .await?;

// Close the trade
let _closed_trade = rest
    .futures_isolated
    .close_trade(trade.id())
    .await?;
  
// Place a new cross order
let _new_order = rest
    .futures_cross
    .place_order(
        TradeSide::Buy,
        Quantity::try_from(1)?, // 1 USD
        TradeExecution::Market,
        None, // client order id
    )
    .await?;

let _close_order = rest.futures_cross.close_position().await?;
```

For more complete authenticated REST API examples, see
[`lnm-sdk/examples/v3_rest_auth.rs`](examples/v3_rest_auth.rs).

### WebSocket API

The SDK implements the **WebSocket API** for real-time market data streaming using [`fastwebsockets`].

**Key Features:**
- **Real-time Data Streaming**: Persistent WebSocket connection with broadcast channels for
  concurrent consumers
- **Connection Reliability**: 
  - Automatic heartbeat monitoring (5-second intervals)
  - Ping/Pong frame handling
  - Graceful disconnection with configurable timeout
  - TLS/SSL encryption via [`tokio-rustls`]
- **Subscription Management**:
  - Prevents conflicting operations on the same channel
  - State tracking for subscribe/unsubscribe requests with server confirmation

```rust
use lnm_sdk::api_v2::{WebSocketChannel, WebSocketClient, WebSocketClientConfig, WebSocketUpdate};

// ...

let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

let client = WebSocketClient::new(WebSocketClientConfig::default(), domain);

let ws = client.connect().await?;
let mut ws_rx = ws.receiver().await?;

ws.subscribe(vec![
    WebSocketChannel::FuturesBtcUsdIndex,
    WebSocketChannel::FuturesBtcUsdLastPrice,
])
.await?;

while let Ok(ws_update) = ws_rx.recv().await {
    match ws_update {
        WebSocketUpdate::ConnectionStatus(status) => {
            println!("{status}");
        }
        WebSocketUpdate::PriceTick(price_tick) => {
            println!("{price_tick}");
        }
        WebSocketUpdate::PriceIndex(price_index) => {
            println!("{price_index}");
        }
    }
}
```

For a more complete WebSocket API example, see [`lnm-sdk/examples/v2_ws.rs`](examples/v2_ws.rs).

## Testing

Some tests require environment variables and are ignored by default. Moreover, said tests must be
run sequentially as they depend on exchange state. The full test suite can be executed by setting
the `LNM_API_*` variables or adding a `.env` file to the project root (a [`.env.template`] file is
available), and then running:

```bash
cargo test -- --include-ignored --test-threads=1
```

## API Reference

+ [LN Markets API v3 Documentation] (recommended)
+ [LN Markets API v2 Documentation] (REST API v2 is deprecated)

## License

*TODO*

## Contribution

*TODO*

[LN Markets]: https://lnmarkets.com/
[`fastwebsockets`]: https://github.com/denoland/fastwebsockets
[`tokio-rustls`]: https://github.com/rustls/tokio-rustls
[`.env.template`]: ../.env.template
[LN Markets API v3 Documentation]: https://api.lnmarkets.com/v3/
[LN Markets API v2 Documentation]: https://docs.lnmarkets.com/api/
