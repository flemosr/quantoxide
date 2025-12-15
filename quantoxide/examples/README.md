# Examples

Example applications demonstrating different ways to use the `quantoxide` crate.

## Prerequisites

All examples require:
- `POSTGRES_DB_URL` - PostgreSQL database connection URL

Synchronization examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain

Live trading examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain
- `LNM_API_V3_KEY` - The LN Markets API v3 key
- `LNM_API_V3_SECRET` - The LN Markets API v3 secret
- `LNM_API_V3_PASSPHRASE` - The LN Markets API v3 passphrase

These environment variables should be set, or a `.env` file should be added in the project root.
A `.env.template` file is available.

## Synchronization

The following examples demonstrate the synchronization engine, which is responsible for determining
the current state of the data stored in the local database, identifying gaps, and fetching the
necessary data from the LN Markets API to remediate them.

### sync_tui

Demonstrates how to run the sync process using its TUI (Terminal User Interface) abstraction, that
automatically handles and displays updates. This is the **recommended approach** for most use cases.

**Usage:**
```bash
cargo run --example sync_tui
```

### sync_direct

Demonstrates direct interaction with the sync process for custom update handling. This approach 
simplifies integration of sync updates into other UIs or processing logic.

**Usage:**
```bash
cargo run --example sync_direct
```

## Backtesting

The following examples demonstrate the backtesting engine, which allows testing trading strategies
against historical data stored in the local database.

### backtest_tui

Demonstrates how to run the backtest process using its TUI (Terminal User Interface) abstraction,
that automatically handles and displays updates. This is the **recommended approach** for most use
cases.

**Usage:**
```bash
cargo run --example backtest_tui
```

### backtest_direct

Demonstrates direct interaction with the backtest process for custom update handling. This approach
simplifies integration of backtest updates into other UIs or processing logic.

**Usage:**
```bash
cargo run --example backtest_direct
```

## Live Trading

The following examples demonstrate the live trading engine, which executes trading strategies in 
real-time against the LN Markets API using live market data and real trading operations.

### live_tui

Demonstrates how to run the live trading process using its TUI (Terminal User Interface) abstraction,
that automatically handles and displays updates. This is the **recommended approach** for most use
cases.

**Usage:**
```bash
cargo run --example live_tui
```

### live_direct

Demonstrates direct interaction with the live trading process for custom update handling. This 
approach simplifies integration of live trading updates into other UIs or processing logic.

**Usage:**
```bash
cargo run --example live_direct
```
