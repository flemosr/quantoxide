# Examples

Example applications demonstrating different ways to use the `quantoxide` crate.

## Quick Templates

Direct source code links for quick reference:

| Category | Raw Operator | Signal Operator / Evaluator | Direct (no TUI) |
|----------|--------------|------------------------------|-----------------|
| **Trade Operator Templates** | [raw.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/operators/raw.rs) | [signal/mod.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/operators/signal/mod.rs) / [signal/evaluator.rs](https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/main/examples/operators/signal/evaluator.rs) | - |
| **Synchronization** | [sync_tui.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/sync_tui.rs) | - | [sync_direct.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/sync_direct.rs) |
| **Backtesting** | [backtest_raw_tui.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/backtest_raw_tui.rs) | [backtest_signal_tui.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/backtest_signal_tui.rs) | [backtest_direct.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/backtest_direct.rs) |
| **Parallel Backtesting** | - | - | [backtest_direct_parallel.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/backtest_direct_parallel.rs) |
| **Live Trading** | [live_raw_tui.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/live_raw_tui.rs) | [live_signal_tui.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/live_signal_tui.rs) | [live_direct.rs](https://raw.githubusercontent.com/flemosr/quantoxide/main/examples/live_direct.rs) |

## Prerequisites

All examples require a running PostgreSQL instance and the following environment variable:
- `POSTGRES_DB_URL` - PostgreSQL database connection URL

Synchronization examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain

Live trading examples require:
- `LNM_API_DOMAIN` - The LN Markets API domain
- `LNM_API_V3_KEY` - The LN Markets API v3 key
- `LNM_API_V3_SECRET` - The LN Markets API v3 secret
- `LNM_API_V3_PASSPHRASE` - The LN Markets API v3 passphrase

These environment variables should be set, or a `.env` file should be added in the project root.
A [`.env.template`](https://github.com/flemosr/quantoxide/blob/main/.env.template) file is
available.

### Setting up PostgreSQL with Docker

To quickly set up a PostgreSQL database for running the examples:

```bash
docker run -d \
  --name quantoxide-postgres \
  -e POSTGRES_PASSWORD=password \
  -p 5432:5432 \
  -v quantoxide-pgdata:/var/lib/postgresql \
  postgres:18.1-bookworm
```

Then the `POSTGRES_DB_URL` environment variable should be set to:
```
postgres://postgres:password@localhost:5432/postgres
```

Useful commands:
+ Stop the container: `docker stop quantoxide-postgres`
+ Start the container: `docker start quantoxide-postgres`
+ Remove the container: `docker rm quantoxide-postgres`
+ Remove the persistent volume: `docker volume rm quantoxide-pgdata`

## Synchronization

The following examples demonstrate the synchronization engine, which is responsible for determining
the current state of the data stored in the local database, identifying gaps, and fetching the
necessary data from the LN Markets API to remediate them.

### sync_tui

Demonstrates how to run the sync process using its TUI (Terminal User Interface) abstraction, that
automatically handles and displays updates. This is the **recommended approach** for most use cases.

Usage:
```bash
cargo run --example sync_tui
```

### sync_direct

Demonstrates direct interaction with the sync process for custom update handling. This approach is 
more LLM-friendly, and simplifies integration of sync updates into other UIs or processing logic.

Usage:
```bash
cargo run --example sync_direct
```

## Backtesting

The following examples demonstrate the backtesting engine, which allows testing trading strategies
against historical data stored in the local database. **Some price history must be available in the 
local database to run the backtest examples**. It can be obtained by running one of the
synchronization examples.

### backtest_raw_tui / backtest_signal_tui

Demonstrates how to run the backtest process using its TUI (Terminal User Interface) abstraction,
that automatically handles and displays updates. This is the **recommended approach** for most use
cases.

Usage with a **raw operator**:
```bash
cargo run --example backtest_raw_tui
```

Usage with a **signal operator** and evaluators:
```bash
cargo run --example backtest_signal_tui
```

### backtest_direct

Demonstrates direct interaction with the backtest process for custom update handling. This approach
is more LLM-friendly, and simplifies integration of backtest updates into other UIs or processing
logic.

Usage:
```bash
cargo run --example backtest_direct -- --start <DATE> --end <DATE> [OPTIONS]
```

Required:
- `--start <DATE>` - Start date in YYYY-MM-DD format
- `--end <DATE>` - End date in YYYY-MM-DD format

Options:
- `--balance <SATS>` - Starting balance in sats (default: 10000000)
- `--rfr-sats <RATE>` - Annual risk-free rate for sats as decimal (default: 0.0)
- `--rfr-usd <RATE>` - Annual risk-free rate for USD as decimal (default: 0.0)

Example:
```bash
cargo run --example backtest_direct -- --start 2025-09-01 --end 2025-12-01 --balance 10000000 --rfr-sats 0.0 --rfr-usd 0.05
```

### backtest_direct_parallel

Demonstrates direct interaction with the parallel backtest engine for custom update handling. This
approach is more LLM-friendly, and simplifies integration of parallel backtest updates into other
UIs or processing logic.

The parallel backtest engine allows multiple operators to be run in parallel over the same time
period and starting balance, while maintaining isolated trade execution state per operator. Candle
management overhead is shared across all operators, making this significantly more efficient than
running separate backtests. Useful for comparing strategies or running parameter sweeps.

Usage:
```bash
cargo run --example backtest_direct_parallel -- --start <DATE> --end <DATE> [OPTIONS]
```

Required:
- `--start <DATE>` - Start date in YYYY-MM-DD format
- `--end <DATE>` - End date in YYYY-MM-DD format

Options:
- `--balance <SATS>` - Starting balance in sats (default: 10000000)
- `--rfr-sats <RATE>` - Annual risk-free rate for sats as decimal (default: 0.0)
- `--rfr-usd <RATE>` - Annual risk-free rate for USD as decimal (default: 0.0)

Example:
```bash
cargo run --example backtest_direct_parallel -- --start 2025-09-01 --end 2025-12-01 --balance 10000000 --rfr-sats 0.0 --rfr-usd 0.05
```

## Live Trading

The following examples demonstrate the live trading engine, which executes trading strategies in 
real-time against the LN Markets API using live market data and real trading operations.

### live_raw_tui / live_signal_tui

Demonstrates how to run the live trading process using its TUI (Terminal User Interface) abstraction,
that automatically handles and displays updates. This is the **recommended approach** for most use
cases.

Usage with a **raw operator**:
```bash
cargo run --example live_raw_tui
```

Usage with a **signal operator** and evaluators:
```bash
cargo run --example live_signal_tui
```

### live_direct

Demonstrates direct interaction with the live trading process for custom update handling. This 
approach is more LLM-friendly, and simplifies integration of live trading updates into other UIs or 
processing logic.

Usage:
```bash
cargo run --example live_direct
```
