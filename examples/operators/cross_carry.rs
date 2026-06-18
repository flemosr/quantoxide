//! Cross-margin carry-trade raw operator shared by the direct and TUI examples.

// Remove during implementation
#![allow(unused)]

use std::{
    num::NonZeroU64,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
};

use async_trait::async_trait;

use quantoxide::{
    error::Result,
    models::{
        CrossLeverage, CrossQuantity, Lookback, MinIterationInterval, OhlcCandleRow, OrderQuantity,
        Percentage, PercentageCapped, Price, SATS_PER_BTC, TradeSide,
    },
    trade::{RawOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

pub trait TradingStateExt {
    fn account_net_value_usd(&self) -> f64;
    fn hedged_value_usd(&self) -> f64;
    fn hedge_drift_usd(&self) -> f64;
    fn hedge_drift_percent(&self) -> f64;
    fn target_hedge_quantity(&self) -> Option<CrossQuantity>;
    fn rebalance_threshold_usd(&self, rebalance_threshold_percent: PercentageCapped) -> f64;
    fn order_for_target_hedge(
        &self,
        rebalance_threshold_percent: PercentageCapped,
    ) -> Option<(TradeSide, OrderQuantity)>;
    fn target_short_liquidation_price(&self, liquidation_buffer: Percentage) -> Option<Price>;
    fn initial_cross_margin_for_hedge(
        &self,
        liquidation_buffer: Percentage,
        fee_perc: PercentageCapped,
    ) -> Result<NonZeroU64>;
    fn cross_margin_deposit_for_hedge(
        &self,
        liquidation_buffer: Percentage,
        fee_perc: PercentageCapped,
    ) -> Option<NonZeroU64>;
}

impl TradingStateExt for TradingState {
    fn account_net_value_usd(&self) -> f64 {
        self.total_net_value() as f64 * self.market_price().as_f64() / SATS_PER_BTC
    }

    fn hedged_value_usd(&self) -> f64 {
        -self.cross_position().quantity() as f64
    }

    fn hedge_drift_usd(&self) -> f64 {
        self.account_net_value_usd() - self.hedged_value_usd()
    }

    fn hedge_drift_percent(&self) -> f64 {
        let account_net_value_usd = self.account_net_value_usd();

        if account_net_value_usd.abs() <= f64::EPSILON {
            0.0
        } else {
            self.hedge_drift_usd() / account_net_value_usd * 100.0
        }
    }

    /// Target short quantity that hedges the account net value in USD.
    fn target_hedge_quantity(&self) -> Option<CrossQuantity> {
        CrossQuantity::try_from(self.account_net_value_usd().floor()).ok()
    }

    fn rebalance_threshold_usd(&self, rebalance_threshold_percent: PercentageCapped) -> f64 {
        self.account_net_value_usd().abs() * rebalance_threshold_percent.as_f64() / 100.0
    }

    /// Returns the order needed to reach the target short hedge, or `None` when the exposure delta
    /// is already within `threshold_usd`.
    fn order_for_target_hedge(
        &self,
        rebalance_threshold_percent: PercentageCapped,
    ) -> Option<(TradeSide, OrderQuantity)> {
        let threshold_usd = self.rebalance_threshold_usd(rebalance_threshold_percent);
        let target_quantity = self.target_hedge_quantity()?.as_i64().checked_neg()?;
        let current_quantity = self.cross_position().quantity();
        let order_quantity = target_quantity
            .checked_sub(current_quantity)?
            .unsigned_abs();

        if (order_quantity as f64) <= threshold_usd {
            return None;
        }

        let side = if target_quantity > current_quantity {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        };
        let quantity = OrderQuantity::try_from(order_quantity).ok()?;

        Some((side, quantity))
    }

    fn target_short_liquidation_price(&self, liquidation_buffer: Percentage) -> Option<Price> {
        self.market_price().apply_gain(liquidation_buffer).ok()
    }

    fn initial_cross_margin_for_hedge(
        &self,
        liquidation_buffer: Percentage,
        fee_perc: PercentageCapped,
    ) -> Result<NonZeroU64> {
        let target_liquidation = self
            .target_short_liquidation_price(liquidation_buffer)
            .ok_or_else(|| "unable to calculate target short liquidation price".to_string())?;
        let target_quantity = self
            .target_hedge_quantity()
            .ok_or("unable to calculate initial cross hedge target")?;

        // The cross account is neutral at initialization, so the collateral diff is the full deposit.
        self.cross_position()
            .est_collateral_diff_for_exposure(
                TradeSide::Sell,
                target_quantity,
                self.market_price(),
                target_liquidation,
                fee_perc,
            )
            .filter(|diff| *diff > 0)
            .map(|diff| NonZeroU64::new(diff as u64).expect("gt 0"))
            .ok_or_else(|| "unable to calculate a positive initial cross margin target".into())
    }

    /// Sats to deposit before placing a hedge order so the target cross exposure keeps its short
    /// liquidation at target and stays above the locked-margin floor, covering the estimated order fee.
    fn cross_margin_deposit_for_hedge(
        &self,
        liquidation_buffer: Percentage,
        fee_perc: PercentageCapped,
    ) -> Option<NonZeroU64> {
        let target_quantity = self.target_hedge_quantity()?;
        let target_liquidation = self.target_short_liquidation_price(liquidation_buffer)?;

        self.cross_position()
            .est_collateral_diff_for_exposure(
                TradeSide::Sell,
                target_quantity,
                self.market_price(),
                target_liquidation,
                fee_perc,
            )
            .filter(|collateral_diff| *collateral_diff > 0)
            .map(|collateral_diff| NonZeroU64::new(collateral_diff as u64).expect("gt 0"))
    }
}

enum OperatorOutput {
    Stdout,
    Tui(Arc<dyn TuiLogger>),
}

/// Cross-margin carry-trade operator.
///
/// The operator deposits enough starting isolated balance into cross margin to place the short
/// liquidation target at the configured percentage above the current market price, opens a short
/// equal to the account NAV in USD, and rebalances whenever hedge drift exceeds the configured
/// percentage of account NAV.
/// During the run, cross collateral is moved to/from the isolated balance when liquidation drifts
/// beyond the configured tolerance.
pub struct CrossCarryOperator {
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    output: OperatorOutput,
    cross_leverage: CrossLeverage,
    rebalance_threshold_percent: PercentageCapped,
    liquidation_buffer: Percentage,
    liq_tolerance: PercentageCapped,
    fee_perc: PercentageCapped,
    initialized: OnceLock<()>,
    rebalance_count: AtomicU64,
}

impl CrossCarryOperator {
    pub fn new(
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
        liquidation_buffer: Percentage,
        liq_tolerance: PercentageCapped,
        fee_perc: PercentageCapped,
    ) -> Box<Self> {
        Self::with_output(
            OperatorOutput::Stdout,
            cross_leverage,
            rebalance_threshold_percent,
            liquidation_buffer,
            liq_tolerance,
            fee_perc,
        )
    }

    pub fn with_logger(
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
        liquidation_buffer: Percentage,
        liq_tolerance: PercentageCapped,
        fee_perc: PercentageCapped,
        logger: Arc<dyn TuiLogger>,
    ) -> Box<Self> {
        Self::with_output(
            OperatorOutput::Tui(logger),
            cross_leverage,
            rebalance_threshold_percent,
            liquidation_buffer,
            liq_tolerance,
            fee_perc,
        )
    }

    fn with_output(
        output: OperatorOutput,
        cross_leverage: CrossLeverage,
        rebalance_threshold_percent: PercentageCapped,
        liquidation_buffer: Percentage,
        liq_tolerance: PercentageCapped,
        fee_perc: PercentageCapped,
    ) -> Box<Self> {
        Box::new(Self {
            trade_executor: OnceLock::new(),
            output,
            cross_leverage,
            rebalance_threshold_percent,
            liquidation_buffer,
            liq_tolerance,
            fee_perc,
            initialized: OnceLock::new(),
            rebalance_count: AtomicU64::new(0),
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

    /// Increments the rebalance counter and returns the new count.
    fn increment_rebalance_count(&self) -> u64 {
        self.rebalance_count.fetch_add(1, AtomicOrdering::Relaxed) + 1
    }

    async fn log(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        match &self.output {
            OperatorOutput::Stdout => println!("{text}"),
            OperatorOutput::Tui(logger) => logger.log(text).await?,
        }
        Ok(())
    }

    async fn adjust_hedge_size(&self, state: &TradingState) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        if let Some(needed_deposit) =
            state.cross_margin_deposit_for_hedge(self.liquidation_buffer, self.fee_perc)
        {
            if state.balance() < needed_deposit.get() {
                self.log(format!(
                    "  Skipping cross hedge order; need {needed_deposit} sats to support the order but only {} sats are available",
                    state.balance()
                ))
                .await?;
                return Ok(());
            }

            let cross_position = trade_executor.cross_deposit(needed_deposit).await?;
            self.log(format!(
                "  Deposited {} sats to support hedge order; cross margin is now {} sats",
                needed_deposit,
                cross_position.margin()
            ))
            .await?;
        }

        let (order_side, order_quantity) = state
            .order_for_target_hedge(self.rebalance_threshold_percent)
            .ok_or_else(|| "unable to evaluate hedge order")?;

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

    async fn adjust_liquidation_price(&self, state: &TradingState) -> Result<()> {
        let Some(current_liq) = state.cross_position().liquidation() else {
            return Ok(());
        };
        let target_liq = state
            .target_short_liquidation_price(self.liquidation_buffer)
            .ok_or_else(|| "unable to calculate target short liquidation price".to_string())?;
        let Some(collateral_delta) = state.cross_position().est_collateral_diff_for_liquidation(
            state.market_price(),
            target_liq,
            self.fee_perc,
        ) else {
            return Ok(());
        };

        let liq_diff_percent =
            ((current_liq.as_f64() - target_liq.as_f64()) / target_liq.as_f64()).abs() * 100.0;
        if liq_diff_percent <= self.liq_tolerance.as_f64() {
            return Ok(());
        }

        let trade_executor = self.trade_executor()?;

        if collateral_delta > 0 {
            let needed_deposit = collateral_delta as u64;
            let deposit = needed_deposit.min(state.balance());
            let Some(deposit) = NonZeroU64::new(deposit) else {
                self.log(format!(
                    "Cross margin is below liquidation target but no isolated balance is available: liquidation ${:.1}, target ${:.1}",
                    current_liq.as_f64(),
                    target_liq.as_f64()
                ))
                .await?;
                return Ok(());
            };

            let cross_position = trade_executor.cross_deposit(deposit).await?;
            self.log(format!(
                "Deposited {} sats to cross margin; liquidation ${:.1}, target ${:.1}, cross margin {} sats",
                deposit,
                cross_position.liquidation().unwrap_or(current_liq).as_f64(),
                target_liq.as_f64(),
                cross_position.margin()
            ))
            .await?;
        } else if collateral_delta < 0 {
            let max_withdrawal = state
                .cross_position()
                .est_free_margin(state.market_price())
                .saturating_sub(1);
            let withdrawal = collateral_delta.unsigned_abs().min(max_withdrawal);
            let Some(withdrawal) = NonZeroU64::new(withdrawal) else {
                return Ok(());
            };

            let cross_position = trade_executor.cross_withdraw(withdrawal).await?;
            self.log(format!(
                "Withdrew {} sats from cross margin; liquidation ${:.1}, target ${:.1}, cross margin {} sats",
                withdrawal,
                cross_position.liquidation().unwrap_or(current_liq).as_f64(),
                target_liq.as_f64(),
                cross_position.margin()
            ))
            .await?;
        }

        Ok(())
    }

    async fn handle_initialization(&self, trade_executor: &Arc<dyn TradeExecutor>) -> Result<bool> {
        if self.is_initialized() {
            return Ok(false);
        }

        self.log("Initializing cross-margin carry trade").await?;

        let starting_state = trade_executor.trading_state().await?;
        let starting_net_value_usd = starting_state.account_net_value_usd();

        self.log(format!(
            "  Starting account net value: {} sats (${starting_net_value_usd:.2})",
            starting_state.total_net_value()
        ))
        .await?;

        if starting_state.balance() == 0 {
            return Err("starting isolated balance must be greater than zero".into());
        }

        let cross_after_leverage = trade_executor
            .cross_set_leverage(self.cross_leverage)
            .await?;
        self.log(format!(
            "  Set cross leverage to {}x",
            cross_after_leverage.leverage().as_u64()
        ))
        .await?;

        let state_after_leverage = trade_executor.trading_state().await?;
        let target_liquidation = state_after_leverage
            .target_short_liquidation_price(self.liquidation_buffer)
            .ok_or_else(|| "unable to calculate target short liquidation price".to_string())?;
        let balance_to_deposit = state_after_leverage
            .initial_cross_margin_for_hedge(self.liquidation_buffer, self.fee_perc)?;
        if balance_to_deposit.get() > state_after_leverage.balance() {
            return Err(format!(
                "initial cross margin target requires {balance_to_deposit} sats, but only {} sats are available",
                state_after_leverage.balance()
            )
            .into());
        }

        let cross_after_deposit = trade_executor.cross_deposit(balance_to_deposit).await?;
        self.log(format!(
            "  Moved {balance_to_deposit} sats from isolated balance into cross margin to target liquidation ${:.1}; cross margin is now {} sats",
            target_liquidation.as_f64(),
            cross_after_deposit.margin()
        ))
        .await?;

        let state_after_deposit = trade_executor.trading_state().await?;
        self.log("  Opening initial short hedge for the full account net value in USD")
            .await?;

        if state_after_deposit
            .order_for_target_hedge(self.rebalance_threshold_percent)
            .is_some()
        {
            self.adjust_hedge_size(&state_after_deposit).await?;
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

        if state
            .order_for_target_hedge(self.rebalance_threshold_percent)
            .is_some()
        {
            let diff_usd = state.hedge_drift_usd();
            let diff_percent = state.hedge_drift_percent();
            let threshold_percent = self.rebalance_threshold_percent;
            let threshold_usd = state.rebalance_threshold_usd(self.rebalance_threshold_percent);
            let rebalance_number = self.increment_rebalance_count();

            self.log(format!(
                "Rebalance #{rebalance_number}: hedge drift {diff_usd:+.2} USD ({diff_percent:+.2}%) exceeded {threshold_percent:.2}% (${threshold_usd:.2})"
            ))
            .await?;

            self.adjust_hedge_size(&state).await?;
        } else {
            self.adjust_liquidation_price(&state).await?;
        }

        Ok(())
    }
}
