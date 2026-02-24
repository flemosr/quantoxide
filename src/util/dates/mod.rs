use chrono::{DateTime, Duration, Local, SubsecRound, Timelike, Utc};

use crate::{
    shared::OhlcResolution,
    sync::{
        LNM_SETTLEMENT_A_END, LNM_SETTLEMENT_B_END, LNM_SETTLEMENT_B_START, LNM_SETTLEMENT_C_START,
        LNM_SETTLEMENT_INTERVAL_8H,
    },
};

pub(crate) trait DateTimeExt {
    fn ceil_sec(&self) -> DateTime<Utc>;

    fn floor_minute(&self) -> DateTime<Utc>;

    fn is_round_minute(&self) -> bool;

    fn format_local_millis(&self) -> String;

    /// Floors this timestamp to the start of its resolution bucket.
    ///
    /// Uses epoch-based bucketing: `floor(timestamp / bucket_size) * bucket_size`.
    fn floor_to_resolution(&self, resolution: OhlcResolution) -> DateTime<Utc>;

    /// Steps back a number of candles from this timestamp.
    ///
    /// Uses fixed durations based on the resolution's minute count.
    fn step_back_candles(&self, resolution: OhlcResolution, candles: u64) -> DateTime<Utc>;

    /// Returns `true` if this time falls on a valid funding settlement grid point.
    ///
    /// Phase A ({08} UTC) before [`LNM_SETTLEMENT_B_START`],
    /// Phase B ({04, 12, 20} UTC) before [`LNM_SETTLEMENT_C_START`],
    /// Phase C ({00, 08, 16} UTC) from [`LNM_SETTLEMENT_C_START`] onward.
    fn is_valid_funding_settlement_time(&self) -> bool;

    /// Rounds up to the next valid funding settlement time (or returns self if already on-grid).
    fn ceil_funding_settlement_time(&self) -> DateTime<Utc>;

    /// Rounds down to the previous valid funding settlement time (or returns self if already on-grid).
    fn floor_funding_settlement_time(&self) -> DateTime<Utc>;

    /// Floors this timestamp to the start of the day (midnight UTC).
    fn floor_day(&self) -> DateTime<Utc>;
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

    fn is_valid_funding_settlement_time(&self) -> bool {
        let hour = self.hour();
        let clean = self.minute() == 0 && self.second() == 0 && self.nanosecond() == 0;
        let interval_hours = LNM_SETTLEMENT_INTERVAL_8H.num_hours() as u32;
        if *self < LNM_SETTLEMENT_B_START {
            clean && hour == 8 // Phase A: {08}
        } else if *self < LNM_SETTLEMENT_C_START {
            clean && hour % interval_hours == 4 // Phase B: {04, 12, 20}
        } else {
            clean && hour.is_multiple_of(interval_hours) // Phase C: {00, 08, 16}
        }
    }

    fn ceil_funding_settlement_time(&self) -> DateTime<Utc> {
        if self.is_valid_funding_settlement_time() {
            return *self;
        }

        // Dead zone A→B: snap to Phase B start.
        if *self > LNM_SETTLEMENT_A_END && *self < LNM_SETTLEMENT_B_START {
            return LNM_SETTLEMENT_B_START;
        }

        // Dead zone B→C: snap to Phase C start.
        if *self > LNM_SETTLEMENT_B_END && *self < LNM_SETTLEMENT_C_START {
            return LNM_SETTLEMENT_C_START;
        }

        // Phase A: ceil to next {08} UTC.
        if *self < LNM_SETTLEMENT_B_START {
            let base = self
                .date_naive()
                .and_hms_opt(8, 0, 0)
                .expect("valid time")
                .and_utc();

            let result = if *self <= base {
                base
            } else {
                base + Duration::hours(24)
            };

            // If ceiling crosses into dead zone A→B, snap to Phase B start.
            return if result > LNM_SETTLEMENT_A_END {
                LNM_SETTLEMENT_B_START
            } else {
                result
            };
        }

        // Phase B / Phase C: ceil to next 8h grid point.
        let interval = LNM_SETTLEMENT_INTERVAL_8H.num_hours() as i32;
        let phase_offset: i32 = if *self < LNM_SETTLEMENT_C_START { 4 } else { 0 };
        let hour = self.hour() as i32;
        let next_slot = ((hour - phase_offset).div_euclid(interval) + 1) * interval + phase_offset;

        let base = self
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid time")
            .and_utc();

        let result = if next_slot >= 24 {
            base + Duration::hours(24 + phase_offset as i64)
        } else {
            base + Duration::hours(next_slot as i64)
        };

        // If ceiling crosses into the dead zone B→C, snap to Phase C start.
        if result > LNM_SETTLEMENT_B_END && result < LNM_SETTLEMENT_C_START {
            LNM_SETTLEMENT_C_START
        } else {
            result
        }
    }

    fn floor_funding_settlement_time(&self) -> DateTime<Utc> {
        if self.is_valid_funding_settlement_time() {
            return *self;
        }

        // Dead zone A→B: floor to last Phase A settlement.
        if *self > LNM_SETTLEMENT_A_END && *self < LNM_SETTLEMENT_B_START {
            return LNM_SETTLEMENT_A_END;
        }

        // Dead zone B→C: floor to last Phase B settlement.
        if *self > LNM_SETTLEMENT_B_END && *self < LNM_SETTLEMENT_C_START {
            return LNM_SETTLEMENT_B_END;
        }

        // Phase A: floor to {08} UTC.
        if *self < LNM_SETTLEMENT_B_START {
            let base = self
                .date_naive()
                .and_hms_opt(8, 0, 0)
                .expect("valid time")
                .and_utc();

            return if *self >= base {
                base
            } else {
                base - Duration::hours(24)
            };
        }

        // Phase B / Phase C: floor to 8h grid point.
        let interval = LNM_SETTLEMENT_INTERVAL_8H.num_hours() as i32;
        let phase_offset: i32 = if *self < LNM_SETTLEMENT_C_START { 4 } else { 0 };
        let hour = self.hour() as i32;
        let slot = (hour - phase_offset).div_euclid(interval) * interval + phase_offset;

        if slot < 0 {
            // Wraps to previous day's last settlement slot
            let prev_day = self.date_naive().pred_opt().expect("valid date");
            prev_day
                .and_hms_opt((24 + slot) as u32, 0, 0)
                .expect("valid time")
                .and_utc()
        } else {
            self.date_naive()
                .and_hms_opt(slot as u32, 0, 0)
                .expect("valid time")
                .and_utc()
        }
    }

    fn floor_day(&self) -> DateTime<Utc> {
        self.date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid time")
            .and_utc()
    }
}

#[cfg(test)]
mod tests;
