# AI Assistant Quickstart Prompt for `quantoxide`

Copy this prompt into your AI assistant when starting a new project with the `quantoxide` crate.

---

We are working with **quantoxide**, a Rust framework for developing, backtesting, and deploying algorithmic trading strategies for Bitcoin futures on LN Markets.

## Overview

**quantoxide** has four core components:

1. **Trade Operator** - Strategy logic that runs at regular intervals with access to trading state
   - Raw Operators: Process OHLC data directly and execute trades (most common)
   - Signal Operators: Delegate OHLC processing to Signal Evaluators (separate components that analyze data and emit signals)

2. **Synchronization** - Fetches and stores historical OHLC candle data from LN Markets into PostgreSQL (required for backtesting)
   - Backfill mode: Historical data
   - Live mode: WebSocket streaming

3. **Backtesting** - Tests strategies against historical data without risking real funds

4. **Live Trading** - Deploys strategies to production with real funds via authenticated LN Markets API

## Workflow

Follow this workflow when helping develop a trading strategy:

1. **Determine the correct quantoxide version and dependencies:**
   
   **Step 1:** Fetch the quantoxide Cargo.toml manifest:
   ```bash
   curl https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/quantoxide/Cargo.toml
   ```
   
   **Step 2:** Check the published version on crates.io:
   ```bash
   cargo search quantoxide
   ```
   
   **Step 3:** Compare versions and use dependencies compatible with the quantoxide manifest:
   - If the published version matches the manifest, use it
   - If there's a mismatch, note it and use compatible dependency versions
   - All dependency versions should be compatible with those in the quantoxide Cargo.toml

2. **Create a project context file (CLAUDE.md or similar)** to maintain consistency across sessions:
   - Propose creating this file after determining versions
   - Include all essential quantoxide context from this prompt, including links to relevant documentation sections and templates
   - Add project-specific sections:
     - Strategy concept, parameters, and implementation decisions
     - Backtest results, performance metrics, and refinement notes
     - Any trade-offs or design choices made during development
   - Update this file as the strategy evolves to prevent hallucinations in future sessions
   - This ensures continuity and provides complete context when resuming work later

3. **Implement and start synchronization FIRST, BEFORE DISCUSSING STRATEGY**, to download historical price data:
   - Fetch the sync template first (see Code Templates section below)
   - Implement a synchronization binary using `SyncEngine`, inside the `bin` directory
   - Ask the user to start the sync process in the background (downloads may take several minutes)
   - Immediately move to strategy discussion while sync runs - don't wait for it to complete

4. **While synchronization runs, discuss strategy:**
   - Ask about trading strategy preferences
   - Suggest simple strategies (Moving Average crossover, RSI, etc.)
   - Document the chosen strategy in the context file

5. **Develop the Trade Operator** implementing the strategy:
   - Fetch the appropriate operator template first (raw or signal, see Code Templates section below)
   - Implement the operator logic
   - Ensure it compiles with `cargo check`

6. **Implement backtest binaries**
   - Fetch the backtest templates first (see Code Templates section below)
   - Implement backtest binaries using `BacktestEngine`, inside the `bin` directory:
     - `backtest_tui`: Interactive backtest with TUI for manual testing and analysis
     - `backtest_cli`: Non-interactive backtest with CLI arguments for AI-driven automated iteration

7. **Wait for synchronization to complete, then backtest:**
   - **For manual workflow:** Instruct the user to run `backtest_tui` for interactive testing and analysis
   - **For AI-driven iteration:** Use `backtest_cli` to automatically run multiple backtests with different parameters
     - Non-interactive execution with results printed to stdout
     - Define optimization objectives
     - Iterate on strategy parameters systematically
     - Log each iteration's results with parameter values
     - Stop when performance plateaus or user intervention is needed
   - Ask which workflow is preferred before proceeding
   - Update the context file with backtest results, parameter choices, and performance insights
   - Reference the examples README for more details on backtest modes

8. **Implement a live trading TUI binary** when strategy is validated:
   - Fetch the live trading template first (see Code Templates section below)
   - Implement using `LiveTradeEngine`, inside the `bin` directory

## Code Templates - DO NOT GUESS THE API

**CRITICAL:** Complete, working templates exist. You MUST fetch them before writing code. Use `curl` to get the full source - do not use summarization tools.

### Fetch Templates

**Step 1:** Get the examples README (contains all template links, PostgreSQL and environment variables setup):
```bash
curl https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/quantoxide/examples/README.md
```

**Step 2:** Fetch specific templates from URLs in the README:
```bash
# Example: Fetch raw operator template
curl https://raw.githubusercontent.com/flemosr/quantoxide/master/quantoxide/examples/operators/raw.rs
```

**Step 3:** Start from templates and modify for the specific strategy. Templates include all necessary boilerplate, imports, and error handling.

## Dependencies

Use these dependencies with versions compatible with the quantoxide Cargo.toml manifest:

```toml
[dependencies]
quantoxide = "<version-from-quantoxide-manifest>"
async-trait = "<version-from-quantoxide-manifest>"
chrono = { version = "<version-from-quantoxide-manifest>", features = ["now"] }
dotenv = "<version-from-quantoxide-manifest>"
tokio = "<version-from-quantoxide-manifest>"
```

**Note:** The quantoxide Cargo.toml uses `edition = "2024"`. Ensure your toolchain supports this edition.

## Important Constraints

- Only **isolated margin futures** are supported (no cross margin)
- Backtesting **does not account for funding fees** (overstates long returns, understates short returns)
- Only **1-minute candle resolution** is currently supported
- This is **alpha software** - bugs may result in loss of assets

## TUI vs Direct Mode

Each engine has two modes:
- **TUI mode**: Built-in terminal interface (recommended for most use cases)
- **Direct mode**: Custom update handling for integration into other UIs or LLM-friendly output (see `backtest_direct`, `sync_direct`, `live_direct` examples)

**IMPORTANT:** Do NOT attempt to run TUI binaries (`*_tui`) directly. TUIs require interactive terminal control and will not work properly when run by an AI agent. Instead:
- For TUI binaries: Implement them, then instruct the user to run them
- For AI-driven automation: Use direct mode binaries which output to stdout

## Coding Guidelines

**DO NOT GUESS APIs** - Always check the official documentation when writing Trade Operator logic:

1. **TradeExecutor API** - When implementing trade execution logic:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/trade/trait.TradeExecutor.html
   - Provides methods for opening/closing positions, checking the trading state, etc.

2. **TradingState API** - When working with the `trading_state` returned by the executor:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/trade/struct.TradingState.html
   - Contains current position info, balance, entry price, leverage, etc.

3. **TradeRunning Trait** - When evaluating individual running trades:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/trade/trait.TradeRunning.html

4. **Validated Models** - Use strongly-typed validated models for type safety:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/models/index.html
   - Includes `OhlcCandleRow`, `Leverage`, `Quantity`, `Price`, `Percentage`, `PercentageCapped`, `Margin`, `TradeSize`, `TradeSide`, etc.
   - These models provide compile-time validation and prevent invalid values

5. **Trade Utilities** - Use built-in utility functions for calculations and validations:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/models/trade_util/index.html
   - Provides functions for PnL calculations, position sizing, margin requirements, etc.
   - Avoid reimplementing common trading calculations

**General coding practices:**
- Use TUI `::log` methods instead of `println!` when TUIs are running
- Include proper error handling with `Result<()>`
- Database schema auto-initializes on first `Database::new()` call

### Quick Reference

#### Trade Sizing

Two ways to specify trade size:

```rust
use quantoxide::models::TradeSize;

// By notional value (USD)
let size = TradeSize::quantity(100)?;  // $100 position

// By margin/collateral (satoshis)
let size = TradeSize::margin(50_000)?; // 50,000 sats collateral
```

#### Stoploss Options

```rust
use quantoxide::{models::{PercentageCapped, Price}, trade::Stoploss};

// Fixed price stoploss
let sl = Stoploss::fixed(Price::try_from(95_000.0)?);

// Trailing percentage stoploss (follows price movement)
let sl = Stoploss::trailing(PercentageCapped::try_from(5)?); // 5% trailing
```

#### Price Adjustments

```rust
use quantoxide::models::{Price, Percentage, PercentageCapped};

let current_price = Price::try_from(100_000.0)?;

// Apply discount (reduce price by percentage)
let discounted = current_price.apply_discount(PercentageCapped::try_from(10.0)?)?; // 90,000.0

// Apply gain (increase price by percentage)
let increased = current_price.apply_gain(Percentage::try_from(20.0)?)?; // 120,000.0
```

## Resources

- **Documentation:** https://docs.rs/quantoxide/latest/quantoxide/
- **Repository:** https://github.com/flemosr/quantoxide/tree/master
- **Main README:** https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/README.md
- **Examples README (START HERE for templates):** https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/quantoxide/examples/README.md
