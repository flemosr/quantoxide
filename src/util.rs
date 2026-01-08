use std::{
    any::Any,
    fmt,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use chrono::{DateTime, Duration, Local, SubsecRound, Timelike, Utc};
use tokio::task::{JoinError, JoinHandle};

use crate::shared::OhlcResolution;

/// A type that can not be instantiated
pub(crate) enum Never {}

pub(crate) trait DateTimeExt {
    fn ceil_sec(&self) -> DateTime<Utc>;

    fn floor_minute(&self) -> DateTime<Utc>;

    fn is_round_minute(&self) -> bool;

    fn format_local_secs(&self) -> String;

    fn format_local_millis(&self) -> String;

    /// Floors this timestamp to the start of its resolution bucket.
    ///
    /// Uses epoch-based bucketing: `floor(timestamp / bucket_size) * bucket_size`.
    fn floor_to_resolution(&self, resolution: OhlcResolution) -> DateTime<Utc>;

    /// Steps back a number of candles from this timestamp.
    ///
    /// Uses fixed durations based on the resolution's minute count.
    fn step_back_candles(&self, resolution: OhlcResolution, candles: u64) -> DateTime<Utc>;
}

impl DateTimeExt for DateTime<Utc> {
    fn ceil_sec(&self) -> DateTime<Utc> {
        let trunc_time_sec = self.trunc_subsecs(0);
        if trunc_time_sec == *self {
            trunc_time_sec
        } else {
            trunc_time_sec + Duration::seconds(1)
        }
    }

    fn floor_minute(&self) -> DateTime<Utc> {
        self.trunc_subsecs(0)
            .with_second(0)
            .expect("second is always valid")
    }

    fn is_round_minute(&self) -> bool {
        *self == self.floor_minute()
    }

    fn format_local_secs(&self) -> String {
        let local_time = self.with_timezone(&Local);
        local_time.format("%Y-%m-%d %H:%M:%S (%Z)").to_string()
    }

    fn format_local_millis(&self) -> String {
        let local_time = self.with_timezone(&Local);
        local_time.format("%Y-%m-%d %H:%M:%S.%3f (%Z)").to_string()
    }

    fn floor_to_resolution(&self, resolution: OhlcResolution) -> DateTime<Utc> {
        let secs_per_bucket = resolution.as_seconds() as i64;
        let floored_timestamp = (self.timestamp() / secs_per_bucket) * secs_per_bucket;
        DateTime::from_timestamp(floored_timestamp, 0).expect("floored timestamp is always valid")
    }

    fn step_back_candles(&self, resolution: OhlcResolution, candles: u64) -> DateTime<Utc> {
        let floored = self.floor_to_resolution(resolution);
        floored - Duration::minutes(resolution.as_minutes() as i64 * candles as i64)
    }
}

/// A wrapper around `tokio::task::JoinHandle` that automatically aborts the task
/// when the wrapper is dropped, while allowing access to the handle.
///
/// This is useful for ensuring that spawned tasks are cleaned up when they go out
/// of scope, preventing resource leaks.
///
/// # Important Notes
///
/// - When dropped, this calls `abort()` on the task, which does **not** run destructors
///   or cleanup code. Tasks should be designed to handle abrupt cancellation.
/// - Implements `Deref` and `DerefMut` for transparent access to `JoinHandle` methods
/// - Implements `Future` so it can be awaited just like a regular `JoinHandle`
///
/// # Examples
///
/// ```ignore
/// use tokio::time;
/// use crate::util::AbortOnDropHandle;
///
/// async fn example() {
///     // Task will be aborted when handle goes out of scope
///     let handle = AbortOnDropHandle::from(tokio::spawn(async {
///         loop {
///             // Long-running work...
///             time::sleep(time::Duration::from_secs(1)).await;
///         }
///     }));
///
///     // Can still await the handle if needed
///     // handle.await.unwrap();
/// } // Task is aborted here
/// ```
#[derive(Debug)]
pub(crate) struct AbortOnDropHandle<T>(JoinHandle<T>);

impl<T> From<JoinHandle<T>> for AbortOnDropHandle<T> {
    fn from(handle: JoinHandle<T>) -> Self {
        Self(handle)
    }
}

impl<T> Deref for AbortOnDropHandle<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AbortOnDropHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Future for AbortOnDropHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Drop for AbortOnDropHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Debug)]
pub struct PanicPayload(String);

impl From<Box<dyn Any + Send>> for PanicPayload {
    fn from(value: Box<dyn Any + Send>) -> Self {
        let panic_msg = if let Some(s) = value.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = value.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic payload".to_string()
        };

        Self(panic_msg)
    }
}

impl fmt::Display for PanicPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    mod floor_to_resolution {
        use super::*;

        // Sub-hourly resolutions

        #[test]
        fn one_minute_already_aligned() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::OneMinute);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );
        }

        #[test]
        fn one_minute_with_seconds() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 45).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::OneMinute);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );
        }

        #[test]
        fn five_minutes_floors_correctly() {
            // 10:32 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 32, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FiveMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );

            // 10:34:59 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 34, 59).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FiveMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );

            // 10:35 -> 10:35
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 35, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FiveMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 35, 0).unwrap()
            );
        }

        #[test]
        fn fifteen_minutes_floors_correctly() {
            // 10:07 -> 10:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 7, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FifteenMinutes);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap());

            // 10:15 -> 10:15
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 15, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FifteenMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 15, 0).unwrap()
            );

            // 10:44 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 44, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FifteenMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );

            // 10:59 -> 10:45
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 59, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FifteenMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 45, 0).unwrap()
            );
        }

        #[test]
        fn thirty_minutes_floors_correctly() {
            // 10:15 -> 10:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 15, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThirtyMinutes);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap());

            // 10:45 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 45, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThirtyMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );
        }

        #[test]
        fn three_minutes_floors_correctly() {
            // 10:05 -> 10:03
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 5, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThreeMinutes);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 3, 0).unwrap());

            // 10:59 -> 10:57
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 59, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThreeMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 57, 0).unwrap()
            );
        }

        #[test]
        fn ten_minutes_floors_correctly() {
            // 10:25 -> 10:20
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 25, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::TenMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 20, 0).unwrap()
            );
        }

        #[test]
        fn forty_five_minutes_floors_correctly() {
            // 10:50 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 50, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FortyFiveMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );

            // 10:44 -> 10:30
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 44, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FortyFiveMinutes);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap()
            );
        }

        // Hourly resolutions

        #[test]
        fn one_hour_floors_correctly() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 35, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::OneHour);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap());
        }

        #[test]
        fn two_hours_floors_correctly() {
            // 11:30 -> 10:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 11, 30, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::TwoHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap());

            // 12:00 -> 12:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::TwoHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap());
        }

        #[test]
        fn three_hours_floors_correctly() {
            // 11:30 -> 09:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 11, 30, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThreeHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap());

            // 23:59 -> 21:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 23, 59, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::ThreeHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 21, 0, 0).unwrap());
        }

        #[test]
        fn four_hours_floors_correctly() {
            // 05:00 -> 04:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 5, 0, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FourHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 4, 0, 0).unwrap());

            // 23:59 -> 20:00
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 23, 59, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::FourHours);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 20, 0, 0).unwrap());
        }

        // Daily resolution

        #[test]
        fn one_day_floors_to_midnight() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 14, 30, 0).unwrap();
            let result = time.floor_to_resolution(OhlcResolution::OneDay);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap());
        }

        // Edge cases

        #[test]
        fn handles_year_boundary() {
            // Jan 1, 2026 00:00:00 should stay as is for daily resolution
            let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

            assert_eq!(
                time.floor_to_resolution(OhlcResolution::OneDay),
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
            );
        }
    }

    mod step_back_candles {
        use super::*;

        // Sub-hourly resolutions

        #[test]
        fn one_minute_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 59).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneMinute, 5);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 25, 0).unwrap()
            );
        }

        #[test]
        fn five_minutes_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 59).unwrap();
            let result = time.step_back_candles(OhlcResolution::FiveMinutes, 3);
            assert_eq!(
                result,
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 15, 0).unwrap()
            );
        }

        #[test]
        fn fifteen_minutes_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 59).unwrap();
            let result = time.step_back_candles(OhlcResolution::FifteenMinutes, 2);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap());
        }

        // Hourly resolutions

        #[test]
        fn one_hour_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneHour, 3);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 7, 0, 0).unwrap());
        }

        #[test]
        fn four_hours_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::FourHours, 2);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 15, 4, 0, 0).unwrap());
        }

        #[test]
        fn hourly_crosses_day_boundary() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 2, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneHour, 5);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 14, 21, 0, 0).unwrap());
        }

        // Daily resolution

        #[test]
        fn one_day_step_back() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneDay, 7);
            assert_eq!(result, Utc.with_ymd_and_hms(2026, 1, 8, 0, 0, 0).unwrap());
        }

        #[test]
        fn daily_crosses_month_boundary() {
            let time = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneDay, 10);
            assert_eq!(result, Utc.with_ymd_and_hms(2025, 12, 26, 0, 0, 0).unwrap());
        }

        // Edge cases

        #[test]
        fn step_back_zero_candles() {
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneHour, 0);
            assert_eq!(result, time);
        }

        #[test]
        fn step_back_large_number() {
            // Jan 15 10:00 floors to Jan 15 00:00, then back 365 days
            let time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap();
            let result = time.step_back_candles(OhlcResolution::OneDay, 365);
            assert_eq!(result, Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap());
        }
    }
}
