//! Template implementation of a `SignalOperator`.

// Remove during implementation
#![allow(unused)]

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    signal::Signal,
    trade::{SignalOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

// Uncomment to enable trade demo
// use quantoxide::{
//     models::{Leverage, PercentageCapped, TradeSize},
//     trade::Stoploss,
// };

pub struct SignalOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    logger: Option<Arc<dyn TuiLogger>>,
}

impl SignalOperatorTemplate {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            logger: None,
        })
    }

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

#[async_trait]
impl SignalOperator for SignalOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if self.trade_executor.set(trade_executor).is_err() {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    async fn process_signal(&self, signal: &Signal) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;
        //
        // NOTE: `println!` and other `stdout`/`stderr` outputs should be avoided when using TUIs,
        // as they would disrupt rendering.

        // To access the current trading state:

        let trading_state: TradingState = trade_executor.trading_state().await?;
        let iteration_time = trading_state.last_tick_time();
        let balance = trading_state.balance();
        let market_price = trading_state.market_price();
        let running_trades_map = trading_state.running_map();
        // Additional information is available, see the `TradingState` docs

        // Evaluate candles. Perform trading operations via trade executor

        // Iterate over running trades
        for ((creation_time, trade_id), (trade, tsl)) in running_trades_map {
            // Example: Check current profit/loss
            let pl = trade.est_pl(market_price);

            // Take action based on trade status

            // trade_executor.close_trade(*trade_id).await?;
        }

        // Uncomment to enable trade demo
        // // If there are no running trades and balance is gte 6000 sats, open a long trade
        // if running_trades_map.is_empty() && balance >= 6_000 {
        //     trade_executor
        //         .open_long(
        //             TradeSize::quantity(1)?, // Size 1 USD. `TradeSize::margin` is also available
        //             Leverage::try_from(6)?,  // Leverage 6x
        //             Some(Stoploss::trailing(PercentageCapped::try_from(5)?)), // 5% trailing stoploss
        //             None,                                                     // No takeprofit
        //         )
        //         .await?;
        // }

        // ...

        Ok(())
    }
}
