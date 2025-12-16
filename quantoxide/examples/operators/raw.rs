//! Template implementation of a `RawOperator`.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{LookbackPeriod, MinIterationInterval, OhlcCandleRow},
    trade::{RawOperator, TradeExecutor},
    tui::TuiLogger,
};

// Uncomment to enable trade demo
// use quantoxide::trade::Stoploss;
// use lnm_sdk::api_v3::models::{Leverage, TradeSize};

pub struct RawOperatorTemplate {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    logger: Option<Arc<dyn TuiLogger>>,
}

impl RawOperatorTemplate {
    #[allow(dead_code)]
    pub fn new(logger: Option<Arc<dyn TuiLogger>>) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            logger,
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
impl RawOperator for RawOperatorTemplate {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if let Err(_) = self.trade_executor.set(trade_executor) {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    fn lookback(&self) -> Option<LookbackPeriod> {
        // None // Return no candles
        // Some(LookbackPeriod::try_from(10).expect("is valid")) // Last 10 candles (1 min resolution)
        Some(LookbackPeriod::MIN) // Last 5 candles (1 min resolution)
    }

    fn min_iteration_interval(&self) -> MinIterationInterval {
        // MinIterationInterval::seconds(10).expect("is valid") // Minimum iteration interval of 10 seconds
        MinIterationInterval::MIN // Minimum iteration interval of 5 seconds
    }

    #[allow(unused_variables)]
    async fn iterate(&self, candles: &[OhlcCandleRow]) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        // If a TUI `logger` was provided, it can be used to log info in the interface
        // self.log("Logging in the TUI".into()).await?;

        // To access the current trading state:

        let trading_state = trade_executor.trading_state().await?;
        let balance = trading_state.balance();
        let market_price = trading_state.market_price();
        let running_trades_map = trading_state.running_map();
        // Additional information is available

        // Evaluate candles and perform trading operations via trade executor

        // Uncomment to enable trade demo
        // // If there are no running trades and balance is gte 6000 sats, open a long trade
        // if running_trades_map.is_empty() && balance >= 6_000 {
        //     trade_executor
        //         .open_long(
        //             TradeSize::quantity(1)?, // Size 1 USD. `TradeSize::margin` is also available
        //             Leverage::try_from(6)?,  // Leverage 6x
        //             Some(Stoploss::trailing(5.try_into()?)), // 5% trailing stoploss
        //             None,                    // No takeprofit
        //         )
        //         .await?;
        // }

        // ...

        Ok(())
    }
}
