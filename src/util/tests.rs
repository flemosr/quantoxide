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

mod is_valid_funding_settlement_time {
    use super::*;
    use crate::sync::LNM_SETTLEMENT_A_START;

    // Phase A grid: {08} UTC (before 2021-12-07T20:00Z)

    #[test]
    fn phase_a_valid_08() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_00() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 0, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_04() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 4, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_12() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 12, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_16() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 16, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_20() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 20, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_invalid_with_minutes() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 8, 30, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn settlement_start_is_valid() {
        assert!(LNM_SETTLEMENT_A_START.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_a_end_is_valid() {
        assert!(LNM_SETTLEMENT_A_END.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_start_is_valid() {
        assert!(LNM_SETTLEMENT_B_START.is_valid_funding_settlement_time());
    }

    #[test]
    fn dead_zone_ab_midpoint_is_invalid() {
        // 2021-12-07 12:00 is in the dead zone between Phase A and Phase B
        let time = Utc.with_ymd_and_hms(2021, 12, 7, 12, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    // Phase B grid: {04, 12, 20} UTC (before 2025-04-11T16:00Z)

    #[test]
    fn phase_b_valid_04() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_valid_12() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_valid_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 20, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_invalid_00() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_invalid_08() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 8, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_invalid_16() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 16, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_invalid_with_minutes() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 4, 30, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_b_invalid_with_seconds() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 1).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    // Phase C grid: {00, 08, 16} UTC (from 2025-04-11T16:00Z onward)

    #[test]
    fn phase_c_valid_00() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_valid_08() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_valid_16() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 16, 0, 0).unwrap();
        assert!(time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_invalid_04() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 4, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_invalid_12() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_invalid_20() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }

    // Boundary: the exact transition points

    #[test]
    fn phase_b_end_is_valid() {
        // 2025-04-11T04:00Z is the last Phase B settlement
        assert!(LNM_SETTLEMENT_B_END.is_valid_funding_settlement_time());
    }

    #[test]
    fn phase_c_start_is_valid() {
        // 2025-04-11T16:00Z is the first Phase C settlement
        assert!(LNM_SETTLEMENT_C_START.is_valid_funding_settlement_time());
    }

    #[test]
    fn dead_zone_midpoint_is_invalid() {
        // 2025-04-11T10:00Z is in the dead zone
        let time = Utc.with_ymd_and_hms(2025, 4, 11, 10, 0, 0).unwrap();
        assert!(!time.is_valid_funding_settlement_time());
    }
}

mod ceil_funding_settlement_time {
    use super::*;

    // Phase A: {08} UTC daily

    #[test]
    fn phase_a_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), time);
    }

    #[test]
    fn phase_a_before_08_ceils_to_08() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 3, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_a_midnight_ceils_to_08() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 0, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_a_after_08_ceils_to_next_day() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 12, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 6, 2, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_a_23_ceils_to_next_day() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 23, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 6, 2, 8, 0, 0).unwrap()
        );
    }

    // Dead zone A→B

    #[test]
    fn dead_zone_ab_ceils_to_phase_b_start() {
        let time = Utc.with_ymd_and_hms(2021, 12, 7, 12, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), LNM_SETTLEMENT_B_START);
    }

    #[test]
    fn phase_a_last_day_after_08_ceils_to_phase_b_start() {
        // 2021-12-07 09:00 is past Phase A end, would ceil to next day 08:00
        // but that's past PHASE_A_END, so should snap to Phase B start
        let time = Utc.with_ymd_and_hms(2021, 12, 7, 9, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), LNM_SETTLEMENT_B_START);
    }

    // Phase B: already on grid

    #[test]
    fn phase_b_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), time);
    }

    // Phase B: between grid points

    #[test]
    fn phase_b_between_04_and_12() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 7, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_between_12_and_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 15, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 20, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_after_20_wraps_to_next_day() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 22, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 2, 4, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_hour_before_offset_wraps_to_04() {
        // hour=2, phase_offset=4: this is the underflow edge case
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 2, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_midnight_ceils_to_04() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_01_ceils_to_04() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 1, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_03_59_ceils_to_04() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 3, 59, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap()
        );
    }

    // Phase C: between grid points

    #[test]
    fn phase_c_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), time);
    }

    #[test]
    fn phase_c_between_00_and_08() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 3, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_c_between_08_and_16() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 1, 16, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_c_between_16_and_00_wraps() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        assert_eq!(
            time.ceil_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap()
        );
    }

    // Dead zone

    #[test]
    fn dead_zone_ceils_to_phase_c_start() {
        let time = Utc.with_ymd_and_hms(2025, 4, 11, 10, 0, 0).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), LNM_SETTLEMENT_C_START);
    }

    #[test]
    fn dead_zone_just_after_phase_b_end() {
        let time = Utc.with_ymd_and_hms(2025, 4, 11, 4, 0, 1).unwrap();
        assert_eq!(time.ceil_funding_settlement_time(), LNM_SETTLEMENT_C_START);
    }
}

mod floor_funding_settlement_time {
    use super::*;

    // Phase A: {08} UTC daily

    #[test]
    fn phase_a_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), time);
    }

    #[test]
    fn phase_a_after_08_floors_to_08() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 15, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 6, 1, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_a_before_08_floors_to_prev_day() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 5, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 5, 31, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_a_midnight_floors_to_prev_day() {
        let time = Utc.with_ymd_and_hms(2021, 6, 1, 0, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2021, 5, 31, 8, 0, 0).unwrap()
        );
    }

    // Dead zone A→B

    #[test]
    fn dead_zone_ab_floors_to_phase_a_end() {
        let time = Utc.with_ymd_and_hms(2021, 12, 7, 14, 0, 0).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), LNM_SETTLEMENT_A_END);
    }

    // Phase B: already on grid

    #[test]
    fn phase_b_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 20, 0, 0).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), time);
    }

    // Phase B: between grid points

    #[test]
    fn phase_b_between_04_and_12() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 7, 30, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 4, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_between_12_and_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 15, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_after_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 23, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 3, 1, 20, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_hour_before_offset_wraps_to_previous_day() {
        // hour=2, phase_offset=4: floors to previous day's 20:00
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 2, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 2, 28, 20, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_midnight_floors_to_previous_day_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 2, 28, 20, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_01_floors_to_previous_day_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 1, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 2, 28, 20, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_b_03_59_floors_to_previous_day_20() {
        let time = Utc.with_ymd_and_hms(2025, 3, 1, 3, 59, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2025, 2, 28, 20, 0, 0).unwrap()
        );
    }

    // Phase C: between grid points

    #[test]
    fn phase_c_already_on_grid() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 16, 0, 0).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), time);
    }

    #[test]
    fn phase_c_between_00_and_08() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 5, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_c_between_08_and_16() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap()
        );
    }

    #[test]
    fn phase_c_between_16_and_00() {
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        assert_eq!(
            time.floor_funding_settlement_time(),
            Utc.with_ymd_and_hms(2026, 1, 1, 16, 0, 0).unwrap()
        );
    }

    // Dead zone

    #[test]
    fn dead_zone_floors_to_phase_b_end() {
        let time = Utc.with_ymd_and_hms(2025, 4, 11, 10, 0, 0).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), LNM_SETTLEMENT_B_END);
    }

    #[test]
    fn dead_zone_just_before_phase_c_start() {
        let time = Utc.with_ymd_and_hms(2025, 4, 11, 15, 59, 59).unwrap();
        assert_eq!(time.floor_funding_settlement_time(), LNM_SETTLEMENT_B_END);
    }
}
