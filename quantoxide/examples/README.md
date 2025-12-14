# Examples

Example applications demonstrating different ways to use the `quantoxide` crate.

## Prerequisites

All examples require:
- `POSTGRES_DB_URL` - PostgreSQL database connection URL

Synchronization examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain

These environment variables should be set, or a `.env` file should be added in the project root.

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

*TODO*
