//! Template implementation of a `RawOperator`.

// Remove during implementation
#![allow(unused)]

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{Lookback, MinIterationInterval, OhlcCandleRow, OhlcResolution},
    trade::{RawOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

// Uncomment to enable trade demo
// use quantoxide::{
//     models::{ClientId, Leverage, PercentageCapped, TradeSide, TradeSize},
//     trade::{IsolatedOrderRequest, Stoploss},
// };

enum LogOutput {
    Disabled,
    Stdout,
    Tui(Arc<dyn TuiLogger>),
}

pub struct RawOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    output: LogOutput,
}

impl RawOperatorTemplate {
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

#[async_trait]
impl RawOperator for RawOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if self.trade_executor.set(trade_executor).is_err() {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    fn lookback(&self) -> Option<Lookback> {
        // None // Return no candles

        // Use 15-minute candles with a 10-candle period
        Some(Lookback::new(OhlcResolution::FifteenMinutes, 10).expect("is valid"))
    }

    fn min_iteration_interval(&self) -> MinIterationInterval {
        // MinIterationInterval::seconds(10).expect("is valid") // Minimum iteration interval of 10 seconds
        MinIterationInterval::MIN // Minimum iteration interval of 5 seconds
    }

    async fn iterate(&self, candles: &[OhlcCandleRow]) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        // To access the current trading state:

        let trading_state: TradingState = trade_executor.trading_state().await?;
        let iteration_time = trading_state.last_tick_time();
        let balance = trading_state.balance();
        let market_price = trading_state.market_price();
        let running_trades_map = trading_state.running_map();
        // Additional information is available, see the `TradingState` docs

        self.log(format!(
            "Iteration: time={iteration_time}, market_price={market_price}",
        ))
        .await?;

        // NOTE: direct `stdout`/`stderr` outputs MUST not be used with TUIs, since they disrupt
        // rendering. Use `enable_tui_logger` for TUI-safe logs.

        // Evaluate candles. Perform trading operations via trade executor

        // Iterate over running trades
        for ((creation_time, trade_id), (trade, tsl)) in running_trades_map {
            // Access trade properties

            let client_id = trade.client_id();
            let side = trade.side();
            let pl = trade.est_pl(market_price); // Check current profit/loss
            // ...
            // All `TradeRunning` and `TradeCore` methods are available on `trade`

            // Take action based on trade status

            // trade_executor.isolated_order_close(*trade_id).await?;
        }

        // Uncomment to enable trade demo
        // // If there are no running trades and balance is gte 6000 sats, open a long trade
        // if running_trades_map.is_empty() && balance >= 6_000 {
        //     let request = IsolatedOrderRequest::market(
        //         TradeSide::Buy,
        //         TradeSize::quantity(1)?, // Size 1 USD. `TradeSize::margin` is also available
        //         Leverage::try_from(6)?,  // Leverage 6x
        //     )
        //     // 5% trailing stoploss
        //     .with_stoploss(Stoploss::trailing(PercentageCapped::try_from(5)?))?
        //     // Custom `client_id`
        //     .with_client_id(ClientId::try_from("custom-client-id")?);
        //     let trade_id = trade_executor.isolated_order(request).await?;
        // }

        // ...

        Ok(())
    }
}
