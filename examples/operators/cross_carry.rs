//! Cross-margin carry-trade raw operator shared by the direct and TUI examples.

// Remove during implementation
#![allow(unused)]

use std::{
    num::NonZeroU64,
    sync::{Arc, Mutex, OnceLock},
};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{
        CrossLeverage, Lookback, MinIterationInterval, OhlcCandleRow, PercentageCapped, Quantity,
        SATS_PER_BTC, TradeSide,
    },
    trade::{RawOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

pub fn account_net_value_usd(state: &TradingState) -> f64 {
    state.total_net_value() as f64 * state.market_price().as_f64() / SATS_PER_BTC
}

pub fn hedged_value_usd(state: &TradingState) -> f64 {
    -state.cross_position().quantity() as f64
}

pub fn hedge_drift_usd(state: &TradingState) -> f64 {
    account_net_value_usd(state) - hedged_value_usd(state)
}

pub fn hedge_difference_usd(
    state: &TradingState,
    threshold_usd: f64,
) -> Option<(TradeSide, Quantity)> {
    let diff_usd = hedge_drift_usd(state);

    let mut hedge_order_usd = diff_usd.abs().floor();

    if hedge_order_usd <= threshold_usd {
        return None;
    }

    let side = if diff_usd > 0.0 {
        TradeSide::Sell
    } else {
        TradeSide::Buy
    };

    // If the account is over-hedged, use a long order to reduce the short. Clamp the order so the
    // strategy does not intentionally flip net long when the USD account value is near zero.
    if side == TradeSide::Buy {
        let max_reduction_usd = hedged_value_usd(state).max(0.0);
        hedge_order_usd = hedge_order_usd.min(max_reduction_usd);
    }

    let quantity = Quantity::try_from(hedge_order_usd.round()).ok()?;

    Some((side, quantity))
}

pub fn hedge_difference_percent(state: &TradingState) -> f64 {
    let account_net_value_usd = account_net_value_usd(state);

    if account_net_value_usd.abs() <= f64::EPSILON {
        0.0
    } else {
        hedge_drift_usd(state) / account_net_value_usd * 100.0
    }
}

fn rebalance_threshold_usd(
    state: &TradingState,
    rebalance_threshold_percent: PercentageCapped,
) -> f64 {
    account_net_value_usd(state).abs() * rebalance_threshold_percent.as_f64() / 100.0
}

#[allow(dead_code)]
enum OperatorOutput {
    Stdout,
    Tui(Arc<dyn TuiLogger>),
}

/// Cross-margin carry-trade operator.
///
/// The operator deposits the entire starting isolated balance into cross margin, opens a short
/// equal to the account NAV in USD, and rebalances whenever hedge drift exceeds the configured
/// percentage of account NAV.
pub struct CrossCarryOperator {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    output: OperatorOutput,
    cross_leverage: CrossLeverage,
    rebalance_threshold_percent: PercentageCapped,
    initialized: OnceLock<()>,
    rebalance_count: Mutex<u64>,
}

impl CrossCarryOperator {
    pub fn new(
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
    ) -> Box<Self> {
        Self::with_output(
            OperatorOutput::Stdout,
            cross_leverage,
            rebalance_threshold_percent,
        )
    }

    pub fn with_logger(
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
        logger: Arc<dyn TuiLogger>,
    ) -> Box<Self> {
        Self::with_output(
            OperatorOutput::Tui(logger),
            cross_leverage,
            rebalance_threshold_percent,
        )
    }

    fn with_output(
        output: OperatorOutput,
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
    ) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            output,
            cross_leverage,
            rebalance_threshold_percent,
            initialized: OnceLock::new(),
            rebalance_count: Mutex::new(0),
        })
    }

    fn trade_executor(&self) -> Result<Arc<dyn TradeExecutor>> {
        self.trade_executor
            .get()
            .cloned()
            .ok_or_else(|| "trade executor was not set".into())
    }

    fn is_initialized(&self) -> bool {
        self.initialized.get().is_some()
    }

    fn set_initialized(&self) -> Result<()> {
        if self.initialized.set(()).is_err() {
            return Err("operator was already initialized".into());
        }
        Ok(())
    }

    fn lock_rebalance_count(&self) -> Result<std::sync::MutexGuard<'_, u64>> {
        self.rebalance_count
            .lock()
            .map_err(|_| "rebalance count mutex was poisoned".into())
    }

    fn increment_rebalance_count(&self) -> Result<u64> {
        let mut rebalance_count = self.lock_rebalance_count()?;
        let updated_count = (*rebalance_count)
            .checked_add(1)
            .ok_or("rebalance count overflowed")?;
        *rebalance_count = updated_count;
        Ok(updated_count)
    }

    async fn log(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        match &self.output {
            OperatorOutput::Stdout => println!("{text}"),
            OperatorOutput::Tui(logger) => logger.log(text).await?,
        }
        Ok(())
    }

    async fn adjust_hedge(&self, order_side: TradeSide, order_quantity: Quantity) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        match order_side {
            TradeSide::Sell => {
                let order_id = trade_executor.cross_market_short(order_quantity).await?;
                self.log(format!(
                    "  Placed cross short order {order_id} for ${order_quantity}"
                ))
                .await?;
            }
            TradeSide::Buy => {
                let order_id = trade_executor.cross_market_long(order_quantity).await?;
                self.log(format!(
                    "  Placed cross long order {order_id} for ${order_quantity}"
                ))
                .await?;
            }
        }

        Ok(())
    }

    async fn handle_initialization(&self, trade_executor: &Arc<dyn TradeExecutor>) -> Result<bool> {
        if self.is_initialized() {
            return Ok(false);
        }

        self.log("Initializing cross-margin carry trade").await?;

        let starting_state = trade_executor.trading_state().await?;
        let balance_to_deposit = starting_state.balance();
        let starting_net_value_usd = account_net_value_usd(&starting_state);

        self.log(format!(
            "  Starting account net value: {} sats (${starting_net_value_usd:.2})",
            starting_state.total_net_value()
        ))
        .await?;

        if balance_to_deposit == 0 {
            return Err("starting isolated balance must be greater than zero".into());
        }

        let cross_after_deposit = trade_executor
            .cross_deposit(balance_to_deposit.try_into().expect("not zero"))
            .await?;
        self.log(format!(
            "  Moved {balance_to_deposit} sats from isolated balance into cross margin; cross margin is now {} sats",
            cross_after_deposit.margin()
        ))
        .await?;

        let cross_after_leverage = trade_executor
            .cross_set_leverage(self.cross_leverage)
            .await?;
        self.log(format!(
            "  Set cross leverage to {}x",
            cross_after_leverage.leverage().as_u64()
        ))
        .await?;

        let state_after_deposit = trade_executor.trading_state().await?;
        self.log("  Opening initial short hedge for the full account net value in USD")
            .await?;

        if let Some((order_side, order_quantity)) = hedge_difference_usd(&state_after_deposit, 0.0)
        {
            self.adjust_hedge(order_side, order_quantity).await?;
        }

        self.set_initialized()?;

        Ok(true)
    }
}

#[async_trait]
impl RawOperator for CrossCarryOperator {
    fn set_trade_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        if self.trade_executor.set(trade_executor).is_err() {
            return Err("trade executor was already set".into());
        }
        Ok(())
    }

    fn lookback(&self) -> Option<Lookback> {
        None
    }

    fn min_iteration_interval(&self) -> MinIterationInterval {
        MinIterationInterval::minutes(1).expect("1 minute is valid")
    }

    async fn iterate(&self, _candles: &[OhlcCandleRow]) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        if self.handle_initialization(&trade_executor).await? {
            return Ok(());
        }

        let state = trade_executor.trading_state().await?;
        let threshold_usd = rebalance_threshold_usd(&state, self.rebalance_threshold_percent);

        if let Some((order_side, order_quantity)) = hedge_difference_usd(&state, threshold_usd) {
            let diff_usd = hedge_drift_usd(&state);
            let diff_percent = hedge_difference_percent(&state);
            let threshold_percent = self.rebalance_threshold_percent;

            let rebalance_number = self.increment_rebalance_count()?;
            self.log(format!(
                "Rebalance #{rebalance_number}: hedge drift {diff_usd:+.2} USD ({diff_percent:+.2}%) exceeded {threshold_percent:.2}% (${threshold_usd:.2})"
            ))
            .await?;

            self.adjust_hedge(order_side, order_quantity).await?;
        }

        Ok(())
    }
}
