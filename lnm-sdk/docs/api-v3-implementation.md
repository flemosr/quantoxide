# LNM SDK - API v3 Implementation Status

This document tracks the implementation status of all LN Markets API v3 endpoints in the `lnm-sdk`.

## Legend

- [x] Implemented
- [ ] Pending implementation

---

## REST API

### Utilities Repository

Public endpoints for basic utilities.

- [x] `ping()` - Ping the server
- [x] `time()` - Get the server time

---

### Futures Isolated Repository

Authenticated endpoints for isolated margin futures trading.

#### Read Operations

- [x] `get_open_trades()` - Get all trades that are still open
- [x] `get_running_trades()` - Get all trades that are running
- [x] `get_closed_trades()` - Get closed trades (paginated)
- [x] `get_canceled_trades()` - Get canceled trades (paginated)
- [x] `get_funding_fees()` - Get funding fees paid for isolated trades (paginated)

#### Write Operations

- [x] `new_trade()` - Place a new isolated trade
- [x] `close_trade()` - Close a running trade and realize the PL
- [x] `cancel_trade()` - Cancel an open trade
- [x] `cancel_all_trades()` - Cancel all open trades
- [x] `update_takeprofit()` - Update an open or running trade's takeprofit
- [x] `update_stoploss()` - Update an open or running trade's stoploss
- [x] `add_margin_to_trade()` - Add margin to a running trade
- [x] `cash_in_trade()` - Remove funds from a trade

---

### Futures Cross Repository

Authenticated endpoints for cross margin futures trading.

#### Read Operations

- [x] `get_open_orders()` - Get all cross orders that are still open
- [x] `get_position()` - Get the current cross margin position
- [x] `get_filled_orders()` - Get cross orders that have been filled (paginated)
- [x] `get_funding_fees()` - Get funding fees paid for cross margin position (paginated)
- [x] `get_transfers()` - Get transfers history for cross margin account (paginated)

### Write Operations

- [x] `place_order()` - Place a new cross order
- [x] `close_position()` - Close the running cross margin position
- [x] `cancel_order()` - Cancel an open cross order
- [x] `cancel_all_orders()` - Cancel all open cross orders
- [x] `deposit()` - Deposit funds to cross margin account
- [x] `withdraw()` - Withdraw funds from cross margin account
- [x] `set_leverage()` - Set the leverage of the cross margin position

---

### Futures Data Repository

Public endpoints for futures market data.

- [x] `get_funding_settlements()` - Get funding settlement history (paginated)
- [x] `get_ticker()` - Get the futures ticker
- [x] `get_candles()` - Get candles (OHLCs) history for a given range (paginated)
- [ ] `get_leaderboard()` - Get the 10 first users by P&L (day/week/month/all-time)

---

### Oracle Repository

Public endpoints for oracle price data.

- [x] `get_index()` - Samples index history (paginated)
- [x] `get_last_price()` - Samples last price history (paginated)

---

### Account Repository

Authenticated endpoints for account management.

- [x] `get_account()` - Get account information
- [ ] `get_last_unused_onchain_address()` - Get most recently generated, still unused on-chain address
- [ ] `generate_new_bitcoin_address()` - Generate a new, unused Bitcoin address
- [ ] `get_notifications()` - Get notifications for the current user
- [ ] `mark_notifications_read()` - Mark all notifications as read

---

### Deposits Repository

Authenticated endpoints for deposit operations.

- [ ] `get_internal_deposits()` - Get internal deposits
- [ ] `get_onchain_deposits()` - Get on-chain deposits
- [ ] `get_lightning_deposits()` - Get Lightning deposits
- [ ] `deposit()` - Initiate a new Lightning deposit

---

### Withdrawals Repository

Authenticated endpoints for withdrawal operations.

- [ ] `get_internal_withdrawals()` - Get internal withdrawals
- [ ] `get_onchain_withdrawals()` - Get multiple on-chain withdrawals
- [ ] `get_lightning_withdrawals()` - Get multiple Lightning withdrawals
- [ ] `withdrawal_internal()` - Create a new internal withdrawal
- [ ] `withdrawal_onchain()` - Request a new on-chain withdrawal
- [ ] `withdrawal_lightning()` - Request a new Lightning withdrawal

---

### Synthetic USD Repository

Authenticated and public endpoints for synthetic USD swaps.

#### Authenticated Operations

- [ ] `get_swaps()` - Fetch the user's swaps
- [ ] `create_new_swap()` - Create a new swap

#### Public Operations

- [ ] `get_best_price()` - Get best price

---

### Overall REST Summary

| Repository | Implemented | Total | Percentage |
|-----------|-------------|-------|------------|
| Utilities | 2 | 2 | 100% |
| Futures Isolated | 13 | 13 | 100% |
| Futures Cross | 12 | 12 | 100% |
| Futures Data | 3 | 4 | 75% |
| Oracle | 2 | 2 | 100% |
| Account | 1 | 5 | 20% |
| Deposits | 0 | 4 | 0% |
| Withdrawals | 0 | 6 | 0% |
| Synthetic USD | 0 | 3 | 0% |
| **TOTAL** | **33** | **51** | **64.7%** |

---

## Notes

- REST base URL: `https://api.lnmarkets.com/v3/`
- Working examples are available in:
  - `lnm-sdk/examples/v3_rest_public.rs` (public endpoints)
  - `lnm-sdk/examples/v3_rest_auth.rs` (authenticated endpoints)
