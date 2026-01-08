use chrono::{DateTime, Utc};

use crate::{
    db::models::OhlcCandleRow,
    shared::{Lookback, OhlcResolution},
    util::DateTimeExt,
};

use super::error::{BacktestError, Result};

/// Accumulator for consolidating 1-minute candles into a single resolution bucket.
#[derive(Clone)]
struct BucketAccumulator {
    bucket_time: DateTime<Utc>,
    first_candle_time: DateTime<Utc>,
    last_candle_time: DateTime<Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
    min_created_at: DateTime<Utc>,
    max_updated_at: DateTime<Utc>,
    all_stable: bool,
}

impl BucketAccumulator {
    fn new(bucket_time: DateTime<Utc>) -> Self {
        Self {
            bucket_time,
            first_candle_time: DateTime::<Utc>::MAX_UTC,
            last_candle_time: DateTime::<Utc>::MIN_UTC,
            open: 0.0,
            high: f64::MIN,
            low: f64::MAX,
            close: 0.0,
            volume: 0,
            min_created_at: DateTime::<Utc>::MAX_UTC,
            max_updated_at: DateTime::<Utc>::MIN_UTC,
            all_stable: true,
        }
    }

    fn add_candle(&mut self, candle: &OhlcCandleRow) {
        if candle.time < self.first_candle_time {
            self.first_candle_time = candle.time;
            self.open = candle.open;
        }

        if candle.time > self.last_candle_time {
            self.last_candle_time = candle.time;
            self.close = candle.close;
        }

        self.high = self.high.max(candle.high);
        self.low = self.low.min(candle.low);
        self.volume += candle.volume;

        self.min_created_at = self.min_created_at.min(candle.created_at);
        self.max_updated_at = self.max_updated_at.max(candle.updated_at);

        self.all_stable = self.all_stable && candle.stable;
    }

    fn to_candle_row(&self, is_complete: bool) -> OhlcCandleRow {
        OhlcCandleRow {
            time: self.bucket_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            created_at: self.min_created_at,
            updated_at: self.max_updated_at,
            stable: self.all_stable && is_complete,
        }
    }
}

/// Stateful runtime consolidator for incrementally converting 1-minute candles into target
/// resolution candles.
///
/// This consolidator maintains internal state to avoid reconsolidating already-processed candles.
/// It keeps a buffer of completed consolidated candles and an in-progress bucket for the current
/// time period.
pub(super) struct RuntimeConsolidator {
    /// Lookback configuration containing resolution and period
    lookback: Lookback,
    /// Buffer containing completed candles + current candle (if any)
    /// The last element is the current incomplete bucket when `current_bucket` is Some
    candles: Vec<OhlcCandleRow>,
    /// Current in-progress bucket (stable = false until completed)
    current_bucket: Option<BucketAccumulator>,
}

impl RuntimeConsolidator {
    /// Creates a new runtime consolidator with initial historical candles.
    pub fn new(
        lookback: Lookback,
        initial_candles: &[OhlcCandleRow],
        time_cursor: DateTime<Utc>,
    ) -> Result<Self> {
        let mut consolidator = Self {
            lookback,
            candles: Vec::with_capacity(lookback.period().as_usize() + 1),
            current_bucket: None,
        };

        // For 1-minute resolution, candles map 1:1. Store them directly
        if matches!(lookback.resolution(), OhlcResolution::OneMinute) {
            let mut last_time: Option<DateTime<Utc>> = None;

            for candle in initial_candles {
                if candle.time > time_cursor {
                    continue;
                }

                // Check ordering
                if let Some(prev_time) = last_time {
                    if candle.time < prev_time {
                        return Err(BacktestError::OutOfOrderCandle {
                            candle_time: candle.time,
                            bucket_time: prev_time,
                        });
                    }
                }
                last_time = Some(candle.time);

                let is_complete = candle.time < time_cursor;

                if is_complete {
                    consolidator.candles.push(candle.clone());
                    consolidator.trim_old_candles();
                } else {
                    // Current incomplete candle. Store as the current bucket
                    let mut bucket = BucketAccumulator::new(candle.time);
                    bucket.add_candle(candle);
                    consolidator.candles.push(bucket.to_candle_row(false));
                    consolidator.current_bucket = Some(bucket);
                }
            }
            return Ok(consolidator);
        }

        // Process all candles to build initial state
        for candle in initial_candles {
            if candle.time > time_cursor {
                continue;
            }
            consolidator.push(candle)?;
        }

        Ok(consolidator)
    }

    /// Returns the number of completed candles (excluding current bucket).
    fn completed_count(&self) -> usize {
        if self.current_bucket.is_some() {
            self.candles.len().saturating_sub(1)
        } else {
            self.candles.len()
        }
    }

    /// Trims old completed candles if we exceed the lookback period.
    fn trim_old_candles(&mut self) {
        while self.completed_count() > self.lookback.period().as_usize() {
            self.candles.remove(0);
        }
    }

    /// Finalizes the current bucket, marking it as complete and trimming old candles.
    fn finalize_current_bucket(&mut self) {
        if let Some(current) = self.current_bucket.take() {
            if let Some(last) = self.candles.last_mut() {
                *last = current.to_candle_row(true);
            }
            self.trim_old_candles();
        }
    }

    /// Updates the last element in candles buffer with current bucket state.
    fn sync_current_bucket(&mut self) {
        if let Some(current) = &self.current_bucket {
            let current_candle = current.to_candle_row(false);
            if self.candles.is_empty() {
                self.candles.push(current_candle);
            } else {
                *self.candles.last_mut().unwrap() = current_candle;
            }
        }
    }

    /// Floors a timestamp to the start of its resolution bucket.
    fn floor_to_bucket(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        time.floor_to_resolution(self.lookback.resolution())
    }

    /// Pushes a new 1-minute candle into the consolidator.
    ///
    /// This method incrementally updates the internal state:
    /// - If the candle belongs to the current bucket, it updates that bucket
    /// - If the candle starts a new bucket, it finalizes the previous bucket and starts a new one
    /// - Old buckets outside the lookback window are automatically trimmed
    pub fn push(&mut self, candle: &OhlcCandleRow) -> Result<()> {
        let candle_bucket_time = self.floor_to_bucket(candle.time);

        match &mut self.current_bucket {
            Some(current) if current.bucket_time == candle_bucket_time => {
                current.add_candle(candle);
                self.sync_current_bucket();
                return Ok(());
            }
            Some(current) if candle_bucket_time < current.bucket_time => {
                return Err(BacktestError::OutOfOrderCandle {
                    candle_time: candle.time,
                    bucket_time: current.bucket_time,
                });
            }
            _ => {
                // Finalize current bucket if exists, then start a new bucket
                self.finalize_current_bucket();
            }
        }

        let mut new_bucket = BucketAccumulator::new(candle_bucket_time);
        new_bucket.add_candle(candle);
        self.candles.push(new_bucket.to_candle_row(false));
        self.current_bucket = Some(new_bucket);

        Ok(())
    }

    /// Returns the consolidated candles including the current incomplete bucket.
    pub fn get_candles(&self) -> &[OhlcCandleRow] {
        &self.candles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::TimeZone;

    use crate::shared::Period;

    fn make_candle(time: DateTime<Utc>, price: f64) -> OhlcCandleRow {
        OhlcCandleRow {
            time,
            open: price,
            high: price + 100.0,
            low: price - 100.0,
            close: price,
            volume: 100_000,
            created_at: time,
            updated_at: time,
            stable: true,
        }
    }

    fn empty_consolidator(resolution: OhlcResolution, period: u64) -> RuntimeConsolidator {
        let time_cursor = Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap();
        let period = Period::try_from(period).unwrap();
        let lookback = Lookback::new(resolution, period);
        RuntimeConsolidator::new(lookback, &[], time_cursor).unwrap()
    }

    #[test]
    fn incremental_push_same_bucket() {
        let mut consolidator = empty_consolidator(OhlcResolution::FiveMinutes, 10);

        let base_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();

        // Push 3 candles in the same 5-minute bucket
        consolidator
            .push(&make_candle(base_time, 90_000.0))
            .unwrap();
        consolidator
            .push(&make_candle(
                base_time + chrono::Duration::minutes(1),
                90_100.0,
            ))
            .unwrap();
        consolidator
            .push(&make_candle(
                base_time + chrono::Duration::minutes(2),
                90_200.0,
            ))
            .unwrap();

        let candles = consolidator.get_candles();
        assert_eq!(candles.len(), 1);
        assert!(!candles[0].stable); // Current bucket is incomplete
        assert_eq!(candles[0].open, 90_000.0);
        assert_eq!(candles[0].close, 90_200.0);
    }

    #[test]
    fn incremental_push_new_bucket_finalizes_previous() {
        let mut consolidator = empty_consolidator(OhlcResolution::FiveMinutes, 10);

        let base_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();

        // Push candles in first bucket
        consolidator
            .push(&make_candle(base_time, 90_000.0))
            .unwrap();
        consolidator
            .push(&make_candle(
                base_time + chrono::Duration::minutes(1),
                90_100.0,
            ))
            .unwrap();

        // Push candle in next bucket. Should finalize previous
        consolidator
            .push(&make_candle(
                base_time + chrono::Duration::minutes(5),
                91_000.0,
            ))
            .unwrap();

        let candles = consolidator.get_candles();
        assert_eq!(candles.len(), 2);

        // First bucket should be complete and stable
        assert!(candles[0].stable);
        assert_eq!(
            candles[0].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap()
        );

        // Second bucket is current/incomplete
        assert!(!candles[1].stable);
        assert_eq!(
            candles[1].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 10, 5, 0).unwrap()
        );
    }

    #[test]
    fn new_with_initial_candles() {
        // Create candles from 09:00 to 10:30
        let candles: Vec<OhlcCandleRow> = (0..90)
            .map(|i| {
                let time = Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap()
                    + chrono::Duration::minutes(i);
                make_candle(time, 90_000.0 + i as f64 * 10.0)
            })
            .collect();

        // Time cursor at 10:30 - the 10:00 bucket is incomplete
        let time_cursor = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap();
        let period = Period::try_from(10).unwrap();
        let lookback = Lookback::new(OhlcResolution::OneHour, period);
        let consolidator = RuntimeConsolidator::new(lookback, &candles, time_cursor).unwrap();

        let result = consolidator.get_candles();
        assert_eq!(result.len(), 2);

        // 09:00 bucket should be complete and stable
        assert_eq!(
            result[0].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap()
        );
        assert!(result[0].stable);

        // 10:00 bucket should be incomplete and unstable
        assert_eq!(
            result[1].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap()
        );
        assert!(!result[1].stable);
    }

    #[test]
    fn lookback_trimming() {
        let mut consolidator = empty_consolidator(OhlcResolution::FiveMinutes, 5);

        let base_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();

        // Push candles across 7 buckets (0, 5, 10, 15, 20, 25, 30 minutes)
        // When we push minute 30, the 25-minute bucket is finalized, giving us 6 completed buckets.
        // With lookback=5, we keep 5 completed + 1 current = 6 total.
        for i in 0..7 {
            consolidator
                .push(&make_candle(
                    base_time + chrono::Duration::minutes(i * 5),
                    90_000.0,
                ))
                .unwrap();
        }

        let candles = consolidator.get_candles();

        // 6 completed buckets (10:00 through 10:25) with lookback=5 means
        // 10:00 is trimmed, leaving 10:05, 10:10, 10:15, 10:20, 10:25 completed + 10:30 current
        assert_eq!(candles.len(), 6);

        // First candle should be at 10:05 (10:00 was trimmed)
        assert_eq!(
            candles[0].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 10, 5, 0).unwrap()
        );

        // Last candle should be the current bucket at 10:30
        assert_eq!(
            candles[5].time,
            Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
        );
        assert!(!candles[5].stable);
    }

    #[test]
    fn aggregates_ohlc_correctly() {
        let mut consolidator = empty_consolidator(OhlcResolution::FiveMinutes, 10);

        let base_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();

        let candles = vec![
            OhlcCandleRow {
                time: base_time,
                open: 90_000.0,
                high: 90_500.0,
                low: 89_800.0,
                close: 90_200.0,
                volume: 100_000,
                created_at: base_time,
                updated_at: base_time,
                stable: true,
            },
            OhlcCandleRow {
                time: base_time + chrono::Duration::minutes(1),
                open: 90_200.0,
                high: 91_000.0,
                low: 90_100.0,
                close: 90_800.0,
                volume: 150_000,
                created_at: base_time,
                updated_at: base_time,
                stable: true,
            },
            OhlcCandleRow {
                time: base_time + chrono::Duration::minutes(2),
                open: 90_800.0,
                high: 90_900.0,
                low: 89_500.0,
                close: 89_700.0,
                volume: 200_000,
                created_at: base_time,
                updated_at: base_time,
                stable: true,
            },
        ];

        for candle in &candles {
            consolidator.push(candle).unwrap();
        }

        // Push a candle in the next bucket to finalize the first
        consolidator
            .push(&make_candle(
                base_time + chrono::Duration::minutes(5),
                91_000.0,
            ))
            .unwrap();

        let result = consolidator.get_candles();
        let consolidated = &result[0];

        assert_eq!(consolidated.open, 90_000.0); // Open of first candle
        assert_eq!(consolidated.high, 91_000.0); // Max high
        assert_eq!(consolidated.low, 89_500.0); // Min low
        assert_eq!(consolidated.close, 89_700.0); // Close of last candle
        assert_eq!(consolidated.volume, 450_000); // Sum of volumes
        assert!(consolidated.stable);
    }

    #[test]
    fn one_minute_resolution_passthrough() {
        let base_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();

        let candles: Vec<OhlcCandleRow> = (0..10)
            .map(|i| {
                let time = base_time + chrono::Duration::minutes(i);
                make_candle(time, 90_000.0 + i as f64 * 10.0)
            })
            .collect();

        // Time cursor at 10:08
        let time_cursor = Utc.with_ymd_and_hms(2026, 1, 15, 10, 8, 0).unwrap();
        let period = Period::try_from(5).unwrap();
        let lookback = Lookback::new(OhlcResolution::OneMinute, period);
        let consolidator = RuntimeConsolidator::new(lookback, &candles, time_cursor).unwrap();

        let result = consolidator.get_candles();

        // Should have up to lookback (5) completed + 1 current = 6 candles max
        // But we only have 9 candles up to 10:08, so after trimming to 5 completed:
        // Completed: 10:03, 10:04, 10:05, 10:06, 10:07 (5 candles)
        // Current: 10:08 (1 candle)
        assert_eq!(result.len(), 6);

        // All but last should be stable
        for candle in &result[..5] {
            assert!(candle.stable);
        }
        assert!(!result[5].stable); // Current is unstable
    }
}
