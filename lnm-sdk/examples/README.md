# Examples

Example applications demonstrating different ways to use the `lnm-sdk` crate.

## Prerequisites

All examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain

API v3 authenticated examples (`v3_rest_auth`) require:
- `LNM_API_V3_KEY` - Your API v3 key
- `LNM_API_V3_SECRET` - Your API v3 secret
- `LNM_API_V3_PASSPHRASE` - Your API v3 passphrase

API v2 authenticated examples (`v2_rest_auth`) require:
- `LNM_API_V2_KEY` - Your API v2 key
- `LNM_API_V2_SECRET` - Your API v2 secret
- `LNM_API_V2_PASSPHRASE` - Your API v2 passphrase

These environment variables should be set, or a `.env` file should be added in the project root.

## API v3

The following examples demonstrate the current API v3 interface.

### v3_rest_public

Demonstrates how to use the API v3 REST public client to fetch market data, including utilities
endpoints, futures data, and oracle data.

**Usage:**
```bash
cargo run --example v3_rest_public
```

### v3_rest_auth

Demonstrates how to use the API v3 REST authenticated client to manage both isolated and
cross-margin futures positions, including placing orders, managing margin, and closing positions.

**Usage:**
```bash
cargo run --example v3_rest_auth
```

## API v2 

The following examples demonstrate the API v2 interface.

### v2_rest_public (deprecated)

Demonstrates how to use the API v2 REST public client to fetch market data like futures ticker and
price history.

**Usage:**
```bash
cargo run --example v2_rest_public
```

### v2_rest_auth (deprecated)

Demonstrates how to use the API v2 REST authenticated client to manage trades, including creating,
updating, and closing positions.

**Usage:**
```bash
cargo run --example v2_rest_auth
```

### v2_ws

Demonstrates how to use the API v2 WebSocket client to subscribe to real-time market data channels.

**Usage:**
```bash
cargo run --example v2_ws
```
