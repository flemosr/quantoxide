use std::{num::NonZeroU64, sync::Arc};

use chrono::{Duration, Timelike, Utc};
use tokio::{sync::mpsc, time};

use lnm_sdk::api_v3::{
    RestClient,
    models::{OhlcCandle, OhlcRange},
};

use crate::db::Database;

use super::super::config::{SyncPriceHistoryTaskConfig, SyncProcessConfig};

pub(crate) mod error;
pub(in crate::sync) mod price_history_state;

use error::{Result, SyncPriceHistoryError};
use price_history_state::{DownloadRange, PriceHistoryState};

pub(super) type PriceHistoryStateTransmitter = mpsc::Sender<PriceHistoryState>;

#[derive(Clone)]
pub(super) struct SyncPriceHistoryTask {
    config: SyncPriceHistoryTaskConfig,
    db: Arc<Database>,
    api_rest: Arc<RestClient>,
    history_state_tx: Option<PriceHistoryStateTransmitter>,
}

impl SyncPriceHistoryTask {
    pub fn new(
        config: &SyncProcessConfig,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        history_state_tx: Option<PriceHistoryStateTransmitter>,
    ) -> Self {
        Self {
            config: config.into(),
            db,
            api_rest,
            history_state_tx,
        }
    }

    async fn get_new_ohlc_candles(&self, download_range: DownloadRange) -> Result<Vec<OhlcCandle>> {
        let candles_from = download_range.from();
        let candles_to = download_range.to();

        let limit = match (candles_from, candles_to) {
            (Some(from), to_opt) => {
                // Always get at least 3 candles. It is assumed that `Utc::now()` will be close to
                // the API server time, but small differences should be expected.
                let to_est = to_opt.unwrap_or(Utc::now());
                let exp_candle_qtd = (to_est - from).num_minutes().max(3) as u64;
                let exp_candle_qtd = NonZeroU64::try_from(exp_candle_qtd).expect("must be gte 0");

                exp_candle_qtd.min(self.config.price_history_batch_size())
            }
            _ => self.config.price_history_batch_size(),
        };

        let mut candles: Vec<OhlcCandle> = {
            let mut trials = 0;
            loop {
                time::sleep(self.config.rest_api_cooldown()).await;

                match self
                    .api_rest
                    .futures_data
                    .get_candles(
                        None,
                        None,
                        Some(limit),
                        Some(OhlcRange::OneMinute),
                        candles_to,
                    )
                    .await
                {
                    Ok(ohlc_candle_page) => break ohlc_candle_page.into(),
                    Err(error) => {
                        trials += 1;
                        if trials >= self.config.rest_api_error_max_trials().get() {
                            return Err(SyncPriceHistoryError::RestApiMaxTrialsReached {
                                error,
                                trials: self.config.rest_api_error_max_trials(),
                            });
                        }

                        time::sleep(self.config.rest_api_error_cooldown()).await;
                        continue;
                    }
                };
            }
        };

        if candles.is_empty() {
            return Ok(candles);
        }

        // Validate: times must be rounded to the minute and continuous (1 minute apart, descending)
        for window in candles.windows(2) {
            let [current, next] = window else {
                unreachable!()
            };

            if current.time().second() != 0 || current.time().nanosecond() != 0 {
                return Err(SyncPriceHistoryError::ApiCandlesTimesNotRoundedToMinute);
            }

            if next.time() >= current.time() {
                return Err(SyncPriceHistoryError::ApíCandlesNotOrderedByTimeDesc {
                    inconsistency_at: next.time(),
                });
            }
        }

        let period_start = candles.last().expect("not empty").time();

        // Check the last candle's time is rounded. Not checked when iterating over
        // `candles.windows(2)`. Also handles single candles.
        if period_start.second() != 0 || period_start.nanosecond() != 0 {
            return Err(SyncPriceHistoryError::ApiCandlesTimesNotRoundedToMinute);
        }

        if let Some(time) = candles_from
            && let Some(candle_i) = candles.iter().position(|candle| candle.time() == time)
        {
            // Remove candles with time at `from_observed_time` or before
            let _ = candles.split_off(candle_i);
        }

        Ok(candles)
    }

    async fn partial_download(&self, download_range: DownloadRange) -> Result<()> {
        let new_candles = self.get_new_ohlc_candles(download_range).await?;

        self.db
            .ohlc_candles
            .add_candles(download_range.to(), &new_candles)
            .await?;

        if new_candles.is_empty() {
            match download_range {
                DownloadRange::LowerBound { to } => {
                    // No new entries available before lower bound. Invalid reach config.
                    return Err(
                        SyncPriceHistoryError::ApiCandlesNotAvailableBeforeHistoryStart {
                            history_start: to,
                        },
                    );
                }
                DownloadRange::Gap { from: _, to } => {
                    // No new candles before `to` are currently available, so the gap flag should
                    // be temporarily removed. Missing candles will be flagged by
                    // `flag_missing_candles` next time it runs.
                    self.db.ohlc_candles.remove_gap_flag(to).await?;
                }
                DownloadRange::Latest | DownloadRange::UpperBound { from: _ } => {}
            }
        }

        Ok(())
    }

    async fn handle_history_update(&self, new_history_state: &PriceHistoryState) -> Result<()> {
        if let Some(history_state_tx) = self.history_state_tx.as_ref() {
            history_state_tx
                .send(new_history_state.clone())
                .await
                .map_err(|_| SyncPriceHistoryError::HistoryUpdateHandlerFailed)?;
        }

        Ok(())
    }

    pub async fn backfill(self, flag_gaps_range: Option<Duration>) -> Result<()> {
        if let Some(range) = flag_gaps_range {
            self.db.ohlc_candles.flag_missing_candles(range).await?;
        }

        let mut history_state =
            PriceHistoryState::evaluate_with_reach(&self.db, self.config.price_history_reach())
                .await?;
        self.handle_history_update(&history_state).await?;

        loop {
            let download_range = history_state.next_download_range(true)?;

            self.partial_download(download_range).await?;

            history_state =
                PriceHistoryState::evaluate_with_reach(&self.db, self.config.price_history_reach())
                    .await?;
            self.handle_history_update(&history_state).await?;

            if history_state.has_gaps()? {
                continue;
            }

            if download_range.to().is_none() {
                // Latest entries received. No gaps remain. Backfilling complete.

                if let Some(bound_end) = history_state.bound_end() {
                    self.db.price_ticks.remove_ticks(bound_end).await?;
                }

                return Ok(());
            }
        }
    }

    pub async fn live(self, lookback: Duration) -> Result<()> {
        let history_state = PriceHistoryState::evaluate(&self.db).await?;
        self.handle_history_update(&history_state).await?;

        let initial_download_range = match history_state.bound_end() {
            Some(bound_end) => DownloadRange::UpperBound { from: bound_end },
            None => DownloadRange::Latest,
        };

        self.partial_download(initial_download_range).await?;

        // Now it can be assumed that the history upper bound matches the current time

        loop {
            let history_state = PriceHistoryState::evaluate(&self.db).await?;
            self.handle_history_update(&history_state).await?;

            if let Some(lastest_history_range) = history_state.tail_continuous_duration()
                && lastest_history_range >= lookback
            {
                return Ok(());
            }

            let download_range = history_state.next_download_range(false)?;

            self.partial_download(download_range).await?;
        }
    }
}
