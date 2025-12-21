# AI Assistant Quickstart Prompt for `quantoxide`

Copy this prompt into your AI assistant when starting a new project with the `quantoxide` crate.

---

We are working with **quantoxide**, a Rust framework for developing, backtesting, and deploying algorithmic trading strategies for Bitcoin futures on LN Markets.

## Overview

**quantoxide** has four core components:

1. **Trade Operator** - Strategy logic that runs at regular intervals with access to trading state
   - Raw Operators: Process OHLC data directly and execute trades (most common)
   - Signal Operators: Delegate OHLC processing to Signal Evaluators

2. **Synchronization** - Fetches and stores historical OHLC candle data from LN Markets into PostgreSQL (required for backtesting)
   - Backfill mode: Historical data
   - Live mode: WebSocket streaming

3. **Backtesting** - Tests strategies against historical data without risking real funds

4. **Live Trading** - Deploys strategies to production with real funds via authenticated LN Markets API

## Workflow

Follow this workflow when helping me develop a trading strategy:

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
   - Start by documenting quantoxide version, dependency versions, and Rust edition
   - Add sections for strategy concept, parameters, and implementation decisions
   - Track backtest results, performance metrics, and refinement notes
   - Update this file as the strategy evolves to prevent hallucinations in future sessions
   - This ensures continuity when resuming work later

3. **Implement a synchronization binary** to download historical price data (using `SyncEngine`)

4. **Ask me to start the synchronization process** (it will run in the background)

5. **While synchronization runs, discuss strategy:**
   - Ask about my trading strategy preferences
   - Suggest simple strategies (Moving Average crossover, RSI, etc.)
   - Document the chosen strategy in the context file

6. **Develop the Trade Operator** implementing my strategy. Ensure it compiles with `cargo check`.

7. **Implement a backtest binary** (using `BacktestEngine`)

8. **Wait for synchronization to complete, then backtest:**
   - Run backtests, analyze results
   - Iterate and refine based on performance
   - Update the context file with backtest results and insights

9. **Implement a live trading binary** (using `LiveTradeEngine`) when strategy is validated

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
- **Direct mode**: Custom update handling for integration into other UIs or LLM-friendly output

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
   - Methods: `est_pl()`, `est_max_cash_in()`, `est_max_additional_margin()`

4. **Validated Models** - Use strongly-typed validated models for type safety:
   - Check: https://docs.rs/quantoxide/latest/quantoxide/models/index.html
   - Includes `OhlcCandleRow`, `Leverage`, `Quantity`, `Price`, `Margin`, `TradeSize`, `TradeSide`, etc.
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
use quantoxide::{models::Price, trade::Stoploss};

// Fixed price stoploss
let sl = Stoploss::fixed(Price::try_from(95_000.0)?);

// Trailing percentage stoploss (follows price movement)
let sl = Stoploss::trailing(5.try_into()?); // 5% trailing
```

## Resources

- **Documentation:** https://docs.rs/quantoxide/latest/quantoxide/
- **Repository:** https://github.com/flemosr/quantoxide/tree/master
- **Raw main README:** https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/README.md
- **Raw Examples README:** https://raw.githubusercontent.com/flemosr/quantoxide/refs/heads/master/quantoxide/examples/README.md
