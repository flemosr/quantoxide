# LNM SDK - API v2 Implementation Status

This document tracks the implementation status of all LN Markets API v2 endpoints in the `lnm-sdk`.

## Legend

- [x] Implemented
- [ ] Unimplemented

---

## WebSocket API

The SDK implements the **WebSocket API** for real-time market data streaming using `fastwebsockets`.

### Implementation Status

- [x] **WebSocket Client** - Full JSON-RPC 2.0 compliant client (`src/api_v2/websocket/`)
- [x] **Connection Management** - Automatic reconnection, TLS/SSL support, heartbeat monitoring
- [x] **Subscription Channels**:
  - [x] `futures:btc_usd:index` - Real-time futures BTC/USD index updates
  - [x] `futures:btc_usd:last-price` - Real-time last price and tick direction
- [x] **JSON-RPC Methods**:
  - [x] `v1/public/subscribe` - Subscribe to market data channels
  - [x] `v1/public/unsubscribe` - Unsubscribe from channels

### Key Features

- **Real-time Data Streaming**: Persistent WebSocket connection with broadcast channels for
  concurrent consumers
- **Connection Reliability**: 
  - Automatic heartbeat monitoring (5-second intervals)
  - Ping/Pong frame handling
  - Graceful disconnection with configurable timeout
  - TLS/SSL encryption via `tokio-rustls`
- **Subscription Management**:
  - Prevents conflicting operations on the same channel
  - State tracking for subscribe/unsubscribe requests with server confirmation
- **Error Handling**: Comprehensive error types for connection and API-level errors

---

## REST API

### Futures Repository

Authenticated and public endpoints for futures trading.

#### Trade Operations

- [x] `create_new_trade()` - Create a new trade
- [x] `get_trade()` - Get a specific trade
- [x] `get_trades()` - Get all trades (by status)
- [x] `get_trades_open()` - Get all open trades
- [x] `get_trades_running()` - Get all running trades
- [x] `get_trades_closed()` - Get all closed trades
- [x] `update_trade_stoploss()` - Update a trade's stoploss
- [x] `update_trade_takeprofit()` - Update a trade's takeprofit
- [x] `close_trade()` - Close a trade
- [x] `close_all_trades()` - Close all trades
- [x] `cancel_trade()` - Cancel a trade
- [x] `cancel_all_trades()` - Cancel all trades
- [x] `add_margin()` - Add margin to a trade
- [x] `cash_in()` - Cash-in a trade

#### Market Data

- [ ] `get_market()` - Get futures market information
- [x] `ticker()` - Get ticker data
- [ ] `get_leaderboard()` - Get leaderboard
- [ ] `get_index_history()` - Get index history
- [x] `price_history()` - Get price history
- [ ] `get_fixing_history()` - Get fixing history
- [ ] `get_carry_fees()` - Get carry fees history
- [ ] `get_ohlcs()` - Get OHLC data

---

### Options Repository

Authenticated and public endpoints for options trading.

#### Trade Operations

- [ ] `new_trade()` - Create a new options trade
- [ ] `get_trade()` - Get a specific trade
- [ ] `get_trades()` - Get all trades
- [ ] `update_trade()` - Update a trade
- [ ] `close_trade()` - Close a trade
- [ ] `close_all_trades()` - Close all trades

#### Market Data

- [ ] `get_instrument()` - Get instrument details
- [ ] `get_instruments()` - Get all instruments
- [ ] `get_volatility()` - Get volatility data

---

### Swaps Repository

Authenticated endpoints for swap operations.

- [ ] `create_swap()` - Create a new swap
- [ ] `get_swaps()` - Retrieve all swaps
- [ ] `get_swap()` - Get a specific swap

---

### User Repository

Authenticated endpoints for user account management.

#### Account Management

- [x] `get_user()` - Get user information
- [ ] `update_user()` - Update user settings

#### Deposits

- [ ] `deposit()` - Initiate a deposit
- [ ] `get_deposits()` - List all deposits
- [ ] `get_deposit()` - Get specific deposit details

#### Withdrawals

- [ ] `withdraw()` - Process a withdrawal
- [ ] `get_withdrawals()` - List all withdrawals
- [ ] `get_withdrawal()` - Get specific withdrawal details

#### Transfers & Addresses

- [ ] `transfer()` - Execute internal transfer
- [ ] `get_addresses()` - List bitcoin addresses
- [ ] `create_address()` - Generate a new bitcoin address

---

### Oracle Repository

Public endpoints for oracle price data.

- [ ] `get_index()` - Retrieve index data
- [ ] `get_last_price()` - Get latest price

---

### Notifications Repository

Authenticated endpoints for notifications.

- [ ] `get_notifications()` - Retrieve notifications
- [ ] `mark_notifications_read()` - Mark all notifications as read

---

### Overall REST Summary

| Repository | Implemented | Total | Percentage |
|-----------|-------------|-------|------------|
| Futures | 16 | 18 | 88.9% |
| Options | 0 | 9 | 0% |
| Swaps | 0 | 3 | 0% |
| User | 1 | 10 | 10% |
| Oracle | 0 | 2 | 0% |
| Notifications | 0 | 2 | 0% |
| **TOTAL** | **17** | **44** | **38.6%** |

---

## Notes

- API v2 is the legacy/deprecated LN Markets API. No further development is planned for API v2
  endpoints.
- REST base URL: `https://api.lnmarkets.com/v2`
- WebSocket endpoint: `wss://api.lnmarkets.com`
- Working examples are available in:
  - `lnm-sdk/examples/v2_ws.rs` (WebSocket API usage)
  - `lnm-sdk/examples/v2_rest_public.rs` (public REST endpoints)
  - `lnm-sdk/examples/v2_rest_auth.rs` (authenticated REST endpoints)
