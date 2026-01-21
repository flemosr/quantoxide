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
    trade::{SignalOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

// Uncomment to enable trade demo
// use quantoxide::{
//     models::{Leverage, PercentageCapped, TradeSize},
//     trade::Stoploss,
// };

pub mod evaluator;

pub use evaluator::{SignalAction, SignalTemplate};

/// Example of a simple operator that handles a single signal type directly.
///
/// This demonstrates the simpler case where no unified enum is needed.
#[allow(dead_code)]
pub struct SingleSignalOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
}

#[allow(dead_code)]
impl SingleSignalOperatorTemplate {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
        })
    }

    fn trade_executor(&self) -> Result<&Arc<dyn TradeExecutor>> {
        self.trade_executor
            .get()
            .ok_or_else(|| "trade executor was not set".into())
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
    logger: Option<Arc<dyn TuiLogger>>,
}

impl MultiSignalOperatorTemplate {
    /// Creates a new operator instance.
    ///
    /// Returns a boxed operator ready for use with engines.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            logger: None,
        })
    }

    /// Creates a new operator instance with TUI logging support.
    pub fn with_logger(logger: Arc<dyn TuiLogger>) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            logger: Some(logger),
        })
    }

    fn trade_executor(&self) -> Result<&Arc<dyn TradeExecutor>> {
        if let Some(trade_executor) = self.trade_executor.get() {
            return Ok(trade_executor);
        }
        Err("trade executor was not set".into())
    }

    #[allow(dead_code)]
    async fn log(&self, text: String) -> Result<()> {
        if let Some(logger) = self.logger.as_ref() {
            logger.log(text).await?;
        }
        Ok(())
    }
}

impl Default for MultiSignalOperatorTemplate {
    fn default() -> Self {
        Self {
            trade_executor: OnceLock::new(),
            logger: None,
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

        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;
        //
        // NOTE: `println!` and other `stdout`/`stderr` outputs should be avoided when using TUIs,
        // as they would disrupt rendering.

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
            // Example: Check current profit/loss
            let pl = trade.est_pl(trading_state.market_price());

            // Suppress unused warnings
            let _ = (creation_time, trade_id, tsl);

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
        //         )
        //         .await?;
        // }

        Ok(())
    }
}
