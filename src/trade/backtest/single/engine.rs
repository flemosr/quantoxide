use std::{collections::VecDeque, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;

use crate::{
    db::{Database, models::FundingSettlementRow},
    signal::{Signal, SignalEvaluator},
    sync::{FundingSettlementsState, LNM_SETTLEMENT_A_START, PriceHistoryState},
    util::DateTimeExt,
};

use super::{
    super::{
        super::core::{Raw, RawOperator, SignalOperator, TradeExecutor},
        config::BacktestConfig,
        consolidator::MultiResolutionConsolidator,
        error::{BacktestError, Result},
        executor::SimulatedTradeExecutor,
        state::{
            BacktestReceiver, BacktestStatus, BacktestStatusManager, BacktestTransmitter,
            BacktestUpdate,
        },
    },
    controller::BacktestController,
    operator::OperatorPending,
};

/// Builder for configuring and executing a backtest simulation. Encapsulates the configuration,
/// database connection, operator, and time range for the backtest. The simulation is started when
/// [`start`](Self::start) is called, returning a [`BacktestController`] for monitoring progress.
pub struct BacktestEngine<S: Signal> {
    config: BacktestConfig,
    db: Arc<Database>,
    operator_pending: OperatorPending<S>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    end_time: DateTime<Utc>,
    status_manager: Arc<BacktestStatusManager<BacktestUpdate>>,
    update_tx: BacktestTransmitter,
}

impl<S: Signal> BacktestEngine<S> {
    /// Creates a new backtest engine using signal-based evaluation. Signal evaluators generate
    /// trading signals that are processed by the signal operator to execute trading actions.
    ///
    /// The generic parameter `S` ensures type safety between evaluators and operator. They must
    /// produce and consume the same signal type.
    pub async fn with_signal_operator(
        config: BacktestConfig,
        db: Arc<Database>,
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: Box<dyn SignalOperator<S>>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator_pending = OperatorPending::signal(evaluators, signal_operator.into())?;

        Self::new(
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
        )
        .await
    }

    async fn new(
        config: BacktestConfig,
        db: Arc<Database>,
        operator_pending: OperatorPending<S>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        if !start_time.is_round_minute() || !end_time.is_round_minute() {
            return Err(BacktestError::InvalidTimeRangeNotRounded {
                start_time,
                end_time,
            });
        }

        if end_time <= start_time {
            return Err(BacktestError::InvalidTimeRangeSequence {
                start_time,
                end_time,
            });
        }

        let min_duration = Duration::days(1);
        if end_time - start_time < min_duration {
            let duration_hours = (end_time - start_time).num_hours();
            return Err(BacktestError::InvalidTimeRangeTooShort {
                min_duration,
                duration_hours,
            });
        }

        let max_lookback = operator_pending.max_lookback();

        let price_history_state = PriceHistoryState::evaluate(&db)
            .await
            .map_err(BacktestError::PriceHistoryStateEvaluation)?;

        let lookback_time = if let Some(lookback) = max_lookback {
            start_time.step_back_candles(lookback.resolution(), lookback.period().as_u64() - 1)
        } else {
            start_time
        };

        if !price_history_state
            .is_range_available(lookback_time, end_time)
            .map_err(BacktestError::PriceHistoryStateEvaluation)?
        {
            return Err(BacktestError::PriceHistoryUnavailable {
                lookback_time,
                end_time,
                history_start: price_history_state.bound_start(),
                history_end: price_history_state.bound_end(),
            });
        }

        let settlement_from = start_time.ceil_funding_settlement_time();
        let settlement_to = end_time.floor_funding_settlement_time();

        let funding_settlements_state = FundingSettlementsState::evaluate(&db)
            .await
            .map_err(BacktestError::FundingSettlementsStateEvaluation)?;

        // As of Feb 2026, the LNM API does not provide funding settlement data before
        // `LNM_SETTLEMENT_A_START`, so we only require settlement data for the portion of the
        // backtest that overlaps with the available range. If the entire backtest predates
        // settlement data, no check is needed.
        if settlement_to >= LNM_SETTLEMENT_A_START
            && !funding_settlements_state
                .is_range_available(settlement_from.max(LNM_SETTLEMENT_A_START), settlement_to)
        {
            return Err(BacktestError::FundingSettlementDataUnavailable {
                from: settlement_from,
                to: settlement_to,
                bound_start: funding_settlements_state.bound_start(),
                bound_end: funding_settlements_state.bound_end(),
            });
        }

        let (update_tx, _) = broadcast::channel::<BacktestUpdate>(10_000);

        let status_manager = BacktestStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
            status_manager,
            update_tx,
        })
    }

    /// Returns the start time of the backtest simulation period.
    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    /// Returns the starting balance (in satoshis) for the backtest simulation.
    pub fn start_balance(&self) -> u64 {
        self.start_balance
    }

    /// Returns the end time of the backtest simulation period.
    pub fn end_time(&self) -> DateTime<Utc> {
        self.end_time
    }

    /// Creates a new receiver for subscribing to backtest status and trading state updates.
    pub fn receiver(&self) -> BacktestReceiver {
        self.status_manager.receiver()
    }

    async fn run(self) -> Result<()> {
        self.status_manager.update(BacktestStatus::Starting);

        let buffer_size = self.config.buffer_size() as i64;

        let max_lookback = self
            .operator_pending
            .max_lookback()
            .map(|lb| lb.as_duration())
            .unwrap_or(Duration::zero());

        let buffer_from = self.start_time - max_lookback;
        let buffer_to = buffer_from + Duration::minutes(buffer_size);
        let mut minute_buffer = self
            .db
            .ohlc_candles
            .get_candles(buffer_from, buffer_to)
            .await?;

        // Find the index of the start_time minute candle, or the next available candle
        let start_candle_idx = minute_buffer
            .iter()
            .position(|c| c.time >= self.start_time)
            .ok_or(BacktestError::UnexpectedEmptyBuffer {
                time: self.start_time,
            })?;

        let start_candle = &minute_buffer[start_candle_idx];

        let trades_executor =
            SimulatedTradeExecutor::new(&self.config, start_candle, self.start_balance);

        let settlement_from = self.start_time.ceil_funding_settlement_time();
        let settlement_to = self.end_time.floor_funding_settlement_time();

        let mut settlements: VecDeque<FundingSettlementRow> = self
            .db
            .funding_settlements
            .get_settlements(settlement_from, settlement_to)
            .await?
            .into();

        let mut next_settlement = settlements.pop_front();

        let resolution_to_max_period = self.operator_pending.resolution_to_max_period().clone();

        let mut operator = self
            .operator_pending
            .start(self.start_time, trades_executor.clone())?;

        let mut time_cursor = start_candle.time + Duration::seconds(59);
        let mut minute_cursor_idx = start_candle_idx;

        let mut consolidator = if !resolution_to_max_period.is_empty() {
            let initial_candles = &minute_buffer[..=start_candle_idx];
            Some(MultiResolutionConsolidator::new(
                resolution_to_max_period,
                initial_candles,
                time_cursor,
            )?)
        } else {
            None
        };

        // Send initial trading state at start_time (midnight UTC)
        let initial_state = trades_executor
            .trading_state()
            .await
            .map_err(BacktestError::ExecutorStateEvaluation)?;
        let _ = self.update_tx.send(initial_state.into());

        // Next update will be at end of day (23:59:59), reported as midnight of following day
        let mut send_next_update_at = self.start_time + Duration::days(1) - Duration::seconds(1);

        self.status_manager.update(BacktestStatus::Running);

        loop {
            operator.iterate(time_cursor, consolidator.as_ref()).await?;

            if time_cursor >= send_next_update_at {
                // Report trading state as midnight UTC of each backtested day
                let update_time = send_next_update_at + Duration::seconds(1);
                trades_executor
                    .update_time(update_time)
                    .await
                    .map_err(BacktestError::ExecutorTickUpdate)?;
                let trades_state = trades_executor
                    .trading_state()
                    .await
                    .map_err(BacktestError::ExecutorStateEvaluation)?;

                // Ignore no-receivers errors
                let _ = self.update_tx.send(trades_state.into());

                send_next_update_at += Duration::days(1);
            }

            if time_cursor >= self.end_time - Duration::seconds(1) {
                break;
            }

            minute_cursor_idx += 1;

            // Refetch buffer when exhausted
            if minute_cursor_idx >= minute_buffer.len() {
                let new_buffer_to =
                    (time_cursor + Duration::minutes(buffer_size)).min(self.end_time);

                minute_buffer = self
                    .db
                    .ohlc_candles
                    .get_candles(time_cursor, new_buffer_to)
                    .await?;

                if minute_buffer.is_empty() {
                    return Err(BacktestError::UnexpectedEmptyBuffer { time: time_cursor });
                }

                minute_cursor_idx = 0;
            }

            // Advance time cursor to the end of the next candle's minute (skips gaps in data)
            time_cursor = minute_buffer[minute_cursor_idx].time + Duration::seconds(59);

            // Apply funding settlements that fall within the new time cursor. Applied before
            // `candle_update` so that updated margin/leverage/liquidation are visible to the
            // price-trigger liquidation check.
            while let Some(settlement) = &next_settlement
                && settlement.time <= time_cursor
            {
                trades_executor
                    .apply_funding_settlement(settlement)
                    .await
                    .map_err(BacktestError::FundingSettlementApplication)?;

                next_settlement = settlements.pop_front();
            }

            let next_minute_candle = &minute_buffer[minute_cursor_idx];
            trades_executor
                .candle_update(next_minute_candle)
                .await
                .map_err(BacktestError::ExecutorTickUpdate)?;

            if let Some(consolidator) = &mut consolidator {
                consolidator.push(next_minute_candle)?;
            }
        }

        Ok(())
    }

    /// Starts the backtest simulation and returns a [`BacktestController`] for managing it. This
    /// consumes the engine and spawns the backtest task in the background.
    pub fn start(self) -> Arc<BacktestController> {
        let status_manager = self.status_manager.clone();

        let handle = tokio::spawn(async move {
            let status_manager = self.status_manager.clone();

            let final_backtest_state = match self.run().await {
                Ok(_) => BacktestStatus::Finished,
                Err(e) => BacktestStatus::Failed(Arc::new(e)),
            };

            status_manager.update(final_backtest_state);
        })
        .into();

        BacktestController::new(handle, status_manager)
    }
}

impl BacktestEngine<Raw> {
    /// Creates a new backtest engine using a raw operator. The raw operator directly implements
    /// trading logic without intermediate signal generation.
    pub async fn with_raw_operator(
        config: BacktestConfig,
        db: Arc<Database>,
        raw_operator: Box<dyn RawOperator>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator_pending = OperatorPending::raw(raw_operator.into())?;

        Self::new(
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
        )
        .await
    }
}
