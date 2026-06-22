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
    rebalance_threshold: PercentageCapped,
    liquidation_buffer: Percentage,
    liq_tolerance: PercentageCapped,
    fee_perc: PercentageCapped,
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
            rebalance_threshold: rebalance_threshold_percent,
            liquidation_buffer,
            liq_tolerance,
            fee_perc,
            rebalance_count: AtomicU64::new(0),
        })
    }

    fn trade_executor(&self) -> Result<Arc<dyn TradeExecutor>> {
        self.trade_executor
            .get()
            .cloned()
            .ok_or_else(|| "trade executor was not set".into())
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
            .cross_set_leverage(self.cross_leverage)
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

        if current_leverage != self.cross_leverage {
            self.adjust_cross_leverage(current_leverage).await?;
            return Ok(());
        }

        let carry_status = CarryStatus::evaluate(
            &state,
            self.rebalance_threshold,
            self.liquidation_buffer,
            self.liq_tolerance,
            self.fee_perc,
        )?;
        match carry_status {
            CarryStatus::HedgeImbalanced(imbalance) => {
                let diff_usd = imbalance.drift_usd;
                let diff_percent = imbalance.drift_percent;
                let threshold_percent = self.rebalance_threshold;
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
        state: &TradingState,
        rebalance_threshold: PercentageCapped,
        liquidation_buffer: Percentage,
        liq_tolerance: PercentageCapped,
        fee_perc: PercentageCapped,
    ) -> Result<Self> {
        let balance = state.balance();
        let cross_position = state.cross_position();
        let market_price = state.market_price();
        let target_liquidation = market_price
            .apply_gain(liquidation_buffer)
            .map_err(|error| {
                format!("unable to calculate target short liquidation price: {error}")
            })?;
        let account_net_value_usd =
            state.total_net_value() as f64 * market_price.as_f64() / SATS_PER_BTC;
        let target_hedge = CrossQuantity::try_from(account_net_value_usd.floor())?;

        if let Some(imbalance) = HedgeImbalance::check(
            cross_position,
            market_price,
            target_liquidation,
            target_hedge,
            rebalance_threshold,
            fee_perc,
            balance,
        )? {
            return Ok(Self::HedgeImbalanced(imbalance));
        }

        if let Some(imbalance) = LiquidationImbalance::check(
            cross_position,
            market_price,
            target_liquidation,
            liq_tolerance,
            fee_perc,
            balance,
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
        cross_position: &dyn CrossPositionCore,
        market_price: Price,
        target_liq: Price,
        target_hedge: CrossQuantity,
        rebalance_threshold: PercentageCapped,
        fee_perc: PercentageCapped,
        balance: u64,
    ) -> Result<Option<Self>> {
        let drift_usd = target_hedge.as_i64() + cross_position.quantity();
        let threshold_usd = target_hedge.as_f64() * rebalance_threshold.as_f64() / 100.0;

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
                fee_perc,
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
        cross_position: &dyn CrossPositionCore,
        market_price: Price,
        target_liq: Price,
        liq_tolerance: PercentageCapped,
        fee_perc: PercentageCapped,
        balance: u64,
    ) -> Option<Self> {
        let current_liq = cross_position.liquidation()?;
        let liq_diff_percent =
            ((current_liq.as_f64() - target_liq.as_f64()) / target_liq.as_f64()).abs() * 100.0;
        if liq_diff_percent <= liq_tolerance.as_f64() {
            return None;
        }

        let collateral_delta = cross_position.est_collateral_diff_for_liquidation(
            market_price,
            target_liq,
            fee_perc,
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
