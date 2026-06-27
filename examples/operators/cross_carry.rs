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
    trade::{CrossPositionCore, RawOperator, TradeExecutor, TradingState},
    tui::TuiLogger,
};

/// Configuration for the cross-margin carry-trade operator.
#[derive(Clone, Copy, Debug)]
pub struct CrossCarryOperatorConfig {
    cross_leverage: CrossLeverage,
    rebalance_threshold: PercentageCapped,
    liquidation_buffer: Percentage,
    liq_tolerance: PercentageCapped,
    trade_estimated_fee: PercentageCapped,
}

impl Default for CrossCarryOperatorConfig {
    fn default() -> Self {
        Self {
            cross_leverage: CrossLeverage::bounded(10),
            rebalance_threshold: PercentageCapped::bounded(1.0),
            liquidation_buffer: Percentage::bounded(20.0),
            liq_tolerance: PercentageCapped::bounded(5.0),
            trade_estimated_fee: PercentageCapped::try_from(0.1)
                .expect("must be valid `PercentageCapped`"),
        }
    }
}

impl CrossCarryOperatorConfig {
    /// Returns the cross leverage used by the carry operator.
    pub fn cross_leverage(&self) -> CrossLeverage {
        self.cross_leverage
    }

    /// Returns the hedge drift threshold that triggers a rebalance.
    pub fn rebalance_threshold(&self) -> PercentageCapped {
        self.rebalance_threshold
    }

    /// Returns the target buffer above market for the short liquidation price.
    pub fn liquidation_buffer(&self) -> Percentage {
        self.liquidation_buffer
    }

    /// Returns the liquidation drift tolerance before collateral is adjusted.
    pub fn liq_tolerance(&self) -> PercentageCapped {
        self.liq_tolerance
    }

    /// Returns the estimated trade fee used for collateral calculations.
    pub fn trade_estimated_fee(&self) -> PercentageCapped {
        self.trade_estimated_fee
    }

    /// Sets the cross leverage used by the carry operator.
    ///
    /// Default: `10x`
    pub fn with_cross_leverage(mut self, cross_leverage: CrossLeverage) -> Self {
        self.cross_leverage = cross_leverage;
        self
    }

    /// Sets the hedge drift threshold that triggers a rebalance.
    ///
    /// Default: `1.0%`
    pub fn with_rebalance_threshold(mut self, rebalance_threshold: PercentageCapped) -> Self {
        self.rebalance_threshold = rebalance_threshold;
        self
    }

    /// Sets the target buffer above market for the short liquidation price.
    ///
    /// Default: `20.0%`
    pub fn with_liquidation_buffer(mut self, liquidation_buffer: Percentage) -> Self {
        self.liquidation_buffer = liquidation_buffer;
        self
    }

    /// Sets the liquidation drift tolerance before collateral is adjusted.
    ///
    /// Default: `5.0%`
    pub fn with_liq_tolerance(mut self, liq_tolerance: PercentageCapped) -> Self {
        self.liq_tolerance = liq_tolerance;
        self
    }

    /// Sets the estimated trade fee used for collateral calculations.
    ///
    /// Default: `0.1%`
    pub fn with_trade_estimated_fee(mut self, trade_estimated_fee: PercentageCapped) -> Self {
        self.trade_estimated_fee = trade_estimated_fee;
        self
    }
}

enum LogOutput {
    Disabled,
    Stdout,
    Tui(Arc<dyn TuiLogger>),
}

/// Cross-margin carry-trade operator.
///
/// The operator deposits enough starting isolated balance into cross margin to place the short
/// liquidation target at the configured percentage above the current market price, opens a short
/// equal to the configured percentage of account NAV in USD, and rebalances whenever hedge drift
/// exceeds the configured percentage of the hedge target.
/// During the run, cross collateral is moved to/from the isolated balance when liquidation drifts
/// beyond the configured tolerance.
pub struct CrossCarryOperator {
    config: CrossCarryOperatorConfig,
    hedge_perc: PercentageCapped,
    output: LogOutput,
    trade_executor: OnceLock<Arc<dyn TradeExecutor>>,
    rebalance_count: AtomicU64,
}

impl CrossCarryOperator {
    fn new(
        config: CrossCarryOperatorConfig,
        hedge_perc: PercentageCapped,
        output: LogOutput,
    ) -> Box<Self> {
        Box::new(Self {
            config,
            hedge_perc,
            output,
            trade_executor: OnceLock::new(),
            rebalance_count: AtomicU64::new(0),
        })
    }

    /// Creates a boxed operator with internal logging disabled.
    pub fn boxed(config: CrossCarryOperatorConfig, hedge_perc: PercentageCapped) -> Box<Self> {
        Self::new(config, hedge_perc, LogOutput::Disabled)
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

    fn trade_executor(&self) -> Result<Arc<dyn TradeExecutor>> {
        self.trade_executor
            .get()
            .cloned()
            .ok_or_else(|| "trade executor was not set".into())
    }

    async fn log(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        match &self.output {
            LogOutput::Disabled => {}
            LogOutput::Stdout => println!("{text}"),
            LogOutput::Tui(logger) => logger.log(text).await?,
        }
        Ok(())
    }

    /// Increments the rebalance counter and returns the new count.
    fn increment_rebalance_count(&self) -> u64 {
        self.rebalance_count.fetch_add(1, AtomicOrdering::Relaxed) + 1
    }

    async fn balance_hedge_size(&self, imbalance: &HedgeImbalance) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        if let Some(needed_deposit) = imbalance.needed_deposit {
            if imbalance.balance < needed_deposit.get() {
                self.log(format!(
                    "  Skipping cross hedge order; need {needed_deposit} sats to support the order but only {} sats are available",
                    imbalance.balance
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

        let order_id = trade_executor
            .cross_market(imbalance.order_side, imbalance.order_quantity)
            .await?;
        self.log(format!(
            "  Placed cross {} order {order_id} for ${}",
            imbalance.order_side, imbalance.order_quantity
        ))
        .await?;

        Ok(())
    }

    async fn balance_liquidation(&self, imbalance: &LiquidationImbalance) -> Result<()> {
        let trade_executor = self.trade_executor()?;

        match imbalance.adjustment {
            CollateralAdjustment::Deposit {
                needed,
                available,
                balance,
            } => {
                let Some(deposit) = available else {
                    self.log(format!(
                        "Cross margin is below liquidation target but need {needed} sats and only {balance} sats are available: liquidation ${:.1}, target ${:.1}",
                        imbalance.current_liq.as_f64(),
                        imbalance.target_liq.as_f64()
                    ))
                    .await?;
                    return Ok(());
                };

                let cross_position = trade_executor.cross_deposit(deposit).await?;
                self.log(format!(
                    "Deposited {} sats to cross margin; liquidation ${:.1}, target ${:.1}, cross margin {} sats",
                    deposit,
                    cross_position
                        .liquidation()
                        .unwrap_or(imbalance.current_liq)
                        .as_f64(),
                    imbalance.target_liq.as_f64(),
                    cross_position.margin()
                ))
                .await?;
            }
            CollateralAdjustment::Withdraw { withdrawal } => {
                let cross_position = trade_executor.cross_withdraw(withdrawal).await?;
                self.log(format!(
                    "Withdrew {} sats from cross margin; liquidation ${:.1}, target ${:.1}, cross margin {} sats",
                    withdrawal,
                    cross_position
                        .liquidation()
                        .unwrap_or(imbalance.current_liq)
                        .as_f64(),
                    imbalance.target_liq.as_f64(),
                    cross_position.margin()
                ))
                .await?;
            }
        }

        Ok(())
    }

    async fn adjust_cross_leverage(&self, current_leverage: CrossLeverage) -> Result<()> {
        let trade_executor = self.trade_executor()?;
        let cross_position = trade_executor
            .cross_set_leverage(self.config.cross_leverage())
            .await?;

        self.log(format!(
            "Set cross leverage from {}x to {}x",
            current_leverage.as_u64(),
            cross_position.leverage().as_u64()
        ))
        .await?;

        Ok(())
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
        let state = trade_executor.trading_state().await?;
        let current_leverage = state.cross_position().leverage();

        if current_leverage != self.config.cross_leverage() {
            self.adjust_cross_leverage(current_leverage).await?;
            return Ok(());
        }

        let carry_status = CarryStatus::evaluate(&self.config, self.hedge_perc, &state)?;
        match carry_status {
            CarryStatus::HedgeImbalanced(imbalance) => {
                let diff_usd = imbalance.drift_usd;
                let diff_percent = imbalance.drift_percent;
                let threshold_percent = self.config.rebalance_threshold();
                let threshold_usd = imbalance.threshold_usd;
                let rebalance_number = self.increment_rebalance_count();

                self.log(format!(
                    "Rebalance #{rebalance_number}: hedge drift {diff_usd:+.2} USD ({diff_percent:+.2}%) exceeded {threshold_percent:.2}% (${threshold_usd:.2})"
                ))
                .await?;

                self.balance_hedge_size(&imbalance).await?;
            }
            CarryStatus::LiquidationImbalanced(imbalance) => {
                self.balance_liquidation(&imbalance).await?;
            }
            CarryStatus::Balanced => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CarryStatus {
    HedgeImbalanced(HedgeImbalance),
    LiquidationImbalanced(LiquidationImbalance),
    Balanced,
}

impl CarryStatus {
    pub fn evaluate(
        config: &CrossCarryOperatorConfig,
        hedge_perc: PercentageCapped,
        state: &TradingState,
    ) -> Result<Self> {
        let balance = state.balance();
        let cross_position = state.cross_position();
        let market_price = state.market_price();
        let target_liquidation = market_price
            .apply_gain(config.liquidation_buffer())
            .map_err(|error| {
                format!("unable to calculate target short liquidation price: {error}")
            })?;
        let account_net_value_usd =
            state.total_net_value() as f64 * market_price.as_f64() / SATS_PER_BTC;
        let target_hedge_usd = account_net_value_usd * hedge_perc.as_f64() / 100.0;
        let target_hedge = CrossQuantity::try_from(target_hedge_usd.floor())?;

        if let Some(imbalance) = HedgeImbalance::check(
            config,
            balance,
            cross_position,
            market_price,
            target_liquidation,
            target_hedge,
        )? {
            return Ok(Self::HedgeImbalanced(imbalance));
        }

        if let Some(imbalance) = LiquidationImbalance::check(
            config,
            balance,
            cross_position,
            market_price,
            target_liquidation,
        ) {
            return Ok(Self::LiquidationImbalanced(imbalance));
        }

        Ok(Self::Balanced)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HedgeImbalance {
    threshold_usd: f64,
    drift_usd: f64,
    drift_percent: f64,
    order_side: TradeSide,
    order_quantity: OrderQuantity,
    needed_deposit: Option<NonZeroU64>,
    balance: u64,
}

impl HedgeImbalance {
    fn check(
        config: &CrossCarryOperatorConfig,
        balance: u64,
        cross_position: &dyn CrossPositionCore,
        market_price: Price,
        target_liq: Price,
        target_hedge: CrossQuantity,
    ) -> Result<Option<Self>> {
        let drift_usd = target_hedge.as_i64() + cross_position.quantity();
        let threshold_usd = target_hedge.as_f64() * config.rebalance_threshold().as_f64() / 100.0;

        if drift_usd.unsigned_abs() as f64 <= threshold_usd {
            return Ok(None);
        }

        let order_side = if drift_usd > 0 {
            TradeSide::Sell
        } else {
            TradeSide::Buy
        };
        let order_quantity = if drift_usd.unsigned_abs() > OrderQuantity::MAX.as_u64() {
            OrderQuantity::MAX
        } else {
            OrderQuantity::try_from(drift_usd.unsigned_abs()).expect("gt 0 and <= max")
        };
        let drift_percent = drift_usd as f64 / target_hedge.as_f64() * 100.0;
        let collateral_diff = cross_position
            .est_collateral_diff_for_exposure(
                TradeSide::Sell,
                target_hedge,
                market_price,
                target_liq,
                config.trade_estimated_fee(),
            )
            .ok_or_else(|| {
                format!(
                    "unable to calculate cross collateral required for target short hedge: quantity ${}, liquidation ${:.1}",
                    target_hedge.as_u64(),
                    target_liq.as_f64()
                )
            })?;
        let needed_deposit =
            (collateral_diff > 0).then(|| NonZeroU64::new(collateral_diff as u64).expect("gt 0"));

        Ok(Some(Self {
            threshold_usd,
            drift_usd: drift_usd as f64,
            drift_percent,
            order_side,
            order_quantity,
            needed_deposit,
            balance,
        }))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LiquidationImbalance {
    current_liq: Price,
    target_liq: Price,
    adjustment: CollateralAdjustment,
}

impl LiquidationImbalance {
    fn check(
        config: &CrossCarryOperatorConfig,
        balance: u64,
        cross_position: &dyn CrossPositionCore,
        market_price: Price,
        target_liq: Price,
    ) -> Option<Self> {
        let current_liq = cross_position.liquidation()?;
        let liq_diff_percent =
            ((current_liq.as_f64() - target_liq.as_f64()) / target_liq.as_f64()).abs() * 100.0;
        if liq_diff_percent <= config.liq_tolerance().as_f64() {
            return None;
        }

        let collateral_delta = cross_position.est_collateral_diff_for_liquidation(
            market_price,
            target_liq,
            config.trade_estimated_fee(),
        )?;
        let adjustment = if collateral_delta > 0 {
            let needed = NonZeroU64::new(collateral_delta as u64)?;
            CollateralAdjustment::Deposit {
                needed,
                available: NonZeroU64::new(needed.get().min(balance)),
                balance,
            }
        } else if collateral_delta < 0 {
            let max_withdrawal = cross_position
                .est_free_margin(market_price)
                .saturating_sub(1);
            let withdrawal = NonZeroU64::new(collateral_delta.unsigned_abs().min(max_withdrawal))?;
            CollateralAdjustment::Withdraw { withdrawal }
        } else {
            return None;
        };

        Some(Self {
            current_liq,
            target_liq,
            adjustment,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum CollateralAdjustment {
    Deposit {
        needed: NonZeroU64,
        available: Option<NonZeroU64>,
        balance: u64,
    },
    Withdraw {
        withdrawal: NonZeroU64,
    },
}
