//! Template implementation of a `SignalOperator`.
//!
//! This example demonstrates how to implement a signal operator that processes custom signal types.

// Remove during implementation
#![allow(unused)]

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::ClientId,
    trade::{SignalOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

// Uncomment to enable trade demo
// use quantoxide::{
//     models::{Leverage, PercentageCapped, TradeSize},
//     trade::Stoploss,
// };

enum LogOutput {
    Disabled,
    Stdout,
    Tui(Arc<dyn TuiLogger>),
}

pub mod evaluator;

pub use evaluator::{SignalAction, SignalTemplate};

/// Example of a simple operator that handles a single signal type directly.
///
/// This demonstrates the simpler case where no unified enum is needed.
pub struct SingleSignalOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    output: LogOutput,
}

impl SingleSignalOperatorTemplate {
    fn new(output: LogOutput) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            output,
        })
    }

    /// Creates a boxed operator with internal logging disabled.
    pub fn boxed() -> Box<Self> {
        Self::new(LogOutput::Disabled)
    }

    /// Enables internal logging to stdout.
    ///
    /// Do not use this when running inside a TUI. Direct stdout output corrupts TUI rendering; use
    /// [`Self::enable_tui_logger`] instead.
    pub fn enable_stdout_logger(mut self: Box<Self>) -> Box<Self> {
        self.output = LogOutput::Stdout;
        self
    }

    /// Enables internal logging through a TUI logger.
    pub fn enable_tui_logger(mut self: Box<Self>, logger: Arc<dyn TuiLogger>) -> Box<Self> {
        self.output = LogOutput::Tui(logger);
        self
    }

    fn trade_executor(&self) -> Result<&Arc<dyn TradeExecutor>> {
        self.trade_executor
            .get()
            .ok_or_else(|| "trade executor was not set".into())
    }

    async fn log(&self, text: String) -> Result<()> {
        match &self.output {
            LogOutput::Disabled => {}
            LogOutput::Stdout => println!("{text}"),
            LogOutput::Tui(logger) => logger.log(text).await?,
        }
        Ok(())
    }
}

/// Implementation for single signal type - no enum needed
#[async_trait]
impl SignalOperator<SignalTemplate> for SingleSignalOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        self.trade_executor
            .set(trade_executor)
            .map_err(|_| "trade executor was already set".into())
    }

    async fn process_signal(&self, signal: &SignalTemplate) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        self.log(format!("Processing signal: {signal}")).await?;

        // NOTE: direct `stdout`/`stderr` outputs MUST not be used with TUIs, since they disrupt
        // rendering. Use `enable_tui_logger` for TUI-safe logs.

        // Handle the signal directly - no match needed
        match &signal.action {
            SignalAction::Long { price, strength } => {}
            SignalAction::Short { price, strength } => {}
            SignalAction::CloseLong => {}
            SignalAction::CloseShort => {}
            SignalAction::Wait => {}
        }

        Ok(())
    }
}

/// Example unified signal type for operators that handle multiple signal evaluators running
/// in parallel.
///
/// When using multiple evaluators with different signal types, define a unified enum
/// and implement `From` for each variant.
#[derive(Debug, Clone)]
pub enum SupportedSignal {
    Template(SignalTemplate),
    // Add other signal types as needed:
    // MaCross(MaCrossSignal),
    // Rsi(RsiSignal),
}

impl From<SignalTemplate> for SupportedSignal {
    fn from(signal: SignalTemplate) -> Self {
        Self::Template(signal)
    }
}

impl fmt::Display for SupportedSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupportedSignal::Template(signal) => {
                write!(f, "Template signal at {}: {:?}", signal.time, signal.action)
            }
        }
    }
}

/// A template signal operator that processes unified `SupportedSignal` signals.
pub struct MultiSignalOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    output: LogOutput,
}

impl MultiSignalOperatorTemplate {
    fn new(output: LogOutput) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            output,
        })
    }

    /// Creates a boxed operator with internal logging disabled.
    pub fn boxed() -> Box<Self> {
        Self::new(LogOutput::Disabled)
    }

    /// Enables internal logging to stdout.
    ///
    /// Do not use this when running inside a TUI. Direct stdout output corrupts TUI rendering; use
    /// [`Self::enable_tui_logger`] instead.
    pub fn enable_stdout_logger(mut self: Box<Self>) -> Box<Self> {
        self.output = LogOutput::Stdout;
        self
    }

    /// Enables internal logging through a TUI logger.
    pub fn enable_tui_logger(mut self: Box<Self>, logger: Arc<dyn TuiLogger>) -> Box<Self> {
        self.output = LogOutput::Tui(logger);
        self
    }

    fn trade_executor(&self) -> Result<&Arc<dyn TradeExecutor>> {
        if let Some(trade_executor) = self.trade_executor.get() {
            return Ok(trade_executor);
        }
        Err("trade executor was not set".into())
    }

    async fn log(&self, text: String) -> Result<()> {
        match &self.output {
            LogOutput::Disabled => {}
            LogOutput::Stdout => println!("{text}"),
            LogOutput::Tui(logger) => logger.log(text).await?,
        }
        Ok(())
    }
}

impl Default for MultiSignalOperatorTemplate {
    fn default() -> Self {
        Self {
            trade_executor: OnceLock::new(),
            output: LogOutput::Disabled,
        }
    }
}

#[async_trait]
impl SignalOperator<SupportedSignal> for MultiSignalOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if self.trade_executor.set(trade_executor).is_err() {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    async fn process_signal(&self, signal: &SupportedSignal) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        self.log(format!("Processing signal: {signal}")).await?;

        // NOTE: direct `stdout`/`stderr` outputs MUST not be used with TUIs, since they disrupt
        // rendering. Use `enable_tui_logger` for TUI-safe logs.

        // Access current trading state
        let trading_state: TradingState = trade_executor.trading_state().await?;
        let iteration_time = trading_state.last_tick_time();
        let balance = trading_state.balance();
        let market_price = trading_state.market_price();
        let running_trades_map = trading_state.running_map();
        // Additional information is available, see the `TradingState` docs

        // Handle signals based on their variant
        match signal {
            SupportedSignal::Template(template_signal) => {
                // Process template signal
                match &template_signal.action {
                    SignalAction::Long { price, strength } => {}
                    SignalAction::Short { price, strength } => {}
                    SignalAction::CloseLong => {}
                    SignalAction::CloseShort => {}
                    SignalAction::Wait => {}
                }
            } // Add handlers for other signal types:
              // SupportedSignal::MaCross(ma_signal) => { /* ... */ }
              // SupportedSignal::Rsi(rsi_signal) => { /* ... */ }
        }

        // Iterate over running trades
        for ((creation_time, trade_id), (trade, tsl)) in running_trades_map {
            // Access trade properties

            let client_id = trade.client_id(); // e.g. for signal <-> trade mapping
            let side = trade.side();
            let pl = trade.est_pl(market_price); // Check current profit/loss
            // ...
            // All `TradeRunning` and `TradeCore` methods are available on `trade`

            // Take action based on trade status

            // trade_executor.close_trade(*trade_id).await?;
        }

        // Uncomment to enable trade demo
        // // If there are no running trades and balance is gte 6000 sats, open a long trade
        // if running_trades_map.is_empty() && balance >= 6_000 {
        //     let trade_id = trade_executor
        //         .open_long(
        //             TradeSize::quantity(1)?, // Size 1 USD. `TradeSize::margin` is also available
        //             Leverage::try_from(6)?,  // Leverage 6x
        //             Some(Stoploss::trailing(PercentageCapped::try_from(5)?)), // 5% trailing stoploss
        //             None,                                                     // No takeprofit
        //             Some(ClientId::try_from("custom-client-id")?),            // Custom `client_id`
        //         )
        //         .await?;
        // }

        Ok(())
    }
}
