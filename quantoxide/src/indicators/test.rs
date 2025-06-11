use super::*;

use chrono::TimeZone;

fn datetime_from_timestamp(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn create_partial_entry(time: DateTime<Utc>, value: f64) -> PartialPriceHistoryEntryLOCF {
    PartialPriceHistoryEntryLOCF { time, value }
}

mod get_indicator_calculation_range {
    use super::*;

    #[test]
    fn test_valid_range() {
        let start = datetime_from_timestamp(1000);
        let end = datetime_from_timestamp(2000);

        let calc_start = IndicatorsEvaluator::get_first_required_locf_entry(start);
        let calc_end = IndicatorsEvaluator::get_last_affected_locf_entry(end);

        assert_eq!(calc_start, datetime_from_timestamp(701)); // 1000 - 299
        assert_eq!(calc_end, datetime_from_timestamp(2299)); // 2000 + 299
    }

    #[test]
    fn test_start_equals_end() {
        let time = datetime_from_timestamp(1500);

        let calc_start = IndicatorsEvaluator::get_first_required_locf_entry(time);
        let calc_end = IndicatorsEvaluator::get_last_affected_locf_entry(time);

        assert_eq!(calc_start, datetime_from_timestamp(1201)); // 1500 - 299
        assert_eq!(calc_end, datetime_from_timestamp(1799)); // 1500 + 299
    }
}

mod evaluate {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = IndicatorsEvaluator::evaluate(vec![], datetime_from_timestamp(1000));
        assert!(result.is_err());

        match result.unwrap_err() {
            IndicatorError::EmptyInput => {}
            _ => panic!("Expected EmptyInput error"),
        }
    }

    #[test]
    fn test_first_entry_after_start_time() {
        let entries = vec![
            create_partial_entry(datetime_from_timestamp(1001), 100.0),
            create_partial_entry(datetime_from_timestamp(1002), 101.0),
        ];
        let start_time = datetime_from_timestamp(1000);

        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_err());

        match result.unwrap_err() {
            IndicatorError::InvalidStartTime {
                first_entry_time,
                start_time: s,
            } => {
                assert_eq!(first_entry_time, datetime_from_timestamp(1001));
                assert_eq!(s, start_time);
            }
            _ => panic!("Expected InvalidStartTime error"),
        }
    }

    #[test]
    fn test_last_entry_before_start_time() {
        let entries = vec![
            create_partial_entry(datetime_from_timestamp(998), 100.0),
            create_partial_entry(datetime_from_timestamp(999), 101.0),
        ];
        let start_time = datetime_from_timestamp(1000);

        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_err());

        match result.unwrap_err() {
            IndicatorError::InvalidEndTime {
                last_entry_time,
                start_time: s,
            } => {
                assert_eq!(last_entry_time, datetime_from_timestamp(999));
                assert_eq!(s, start_time);
            }
            _ => panic!("Expected InvalidEndTime error"),
        }
    }

    #[test]
    fn test_non_round_time() {
        let entries = vec![
            create_partial_entry(datetime_from_timestamp(1000), 100.0),
            create_partial_entry(Utc.timestamp_opt(1001, 500_000_000).unwrap(), 101.0), // .5 seconds
        ];
        let start_time = datetime_from_timestamp(1000);

        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_err());

        match result.unwrap_err() {
            IndicatorError::InvalidEntryTime { time } => {
                assert_eq!(
                    time.timestamp_nanos_opt().unwrap() % 1_000_000_000,
                    500_000_000
                );
            }
            _ => panic!("Expected InvalidEntryTime error"),
        }
    }

    #[test]
    fn test_discontinuous_entries() {
        let entries = vec![
            create_partial_entry(datetime_from_timestamp(1000), 100.0),
            create_partial_entry(datetime_from_timestamp(1001), 101.0),
            create_partial_entry(datetime_from_timestamp(1003), 103.0), // Gap at 1002
        ];
        let start_time = datetime_from_timestamp(1000);

        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_err());

        match result.unwrap_err() {
            IndicatorError::DiscontinuousEntries { from, to } => {
                assert_eq!(from, datetime_from_timestamp(1001));
                assert_eq!(to, datetime_from_timestamp(1003));
            }
            _ => panic!("Expected DiscontinuousEntries error"),
        }
    }

    #[test]
    fn test_basic_moving_averages() {
        // Create entries from second 995 to 1010 (16 entries)
        let mut entries = vec![];
        for i in 995..=1010 {
            entries.push(create_partial_entry(
                datetime_from_timestamp(i),
                (i - 995) as f64 + 100.0,
            ));
        }

        let start_time = datetime_from_timestamp(1000);
        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_ok());

        let full_entries = result.unwrap();
        assert_eq!(full_entries.len(), 11); // From 1000 to 1010

        // Check the first returned entry (at time 1000)
        let first = &full_entries[0];
        assert_eq!(first.time, datetime_from_timestamp(1000));
        assert_eq!(first.value, 105.0); // 1000 - 995 + 100

        // MA-5 should be available (we have 6 values: 995-1000)
        assert!(first.ma_5.is_some());
        assert_eq!(first.ma_5.unwrap(), 103.0); // Average of 100,101,102,103,104,105

        // MA-60 and MA-300 should not be available yet
        assert!(first.ma_60.is_none());
        assert!(first.ma_300.is_none());
    }

    #[test]
    fn test_all_moving_averages_populated() {
        // Create 310 entries to ensure all MAs can be calculated
        let mut entries = vec![];
        for i in 0..310 {
            entries.push(create_partial_entry(
                datetime_from_timestamp(1000 + i),
                100.0 + (i as f64 * 0.1), // Gradually increasing values
            ));
        }

        let start_time = datetime_from_timestamp(1300);
        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_ok());

        let full_entries = result.unwrap();
        assert_eq!(full_entries.len(), 10); // From 1300 to 1309

        // Check that all MAs are populated
        for entry in &full_entries {
            assert!(entry.ma_5.is_some());
            assert!(entry.ma_60.is_some());
            assert!(entry.ma_300.is_some());
        }

        // Verify MA values for the first entry (at time 1300)
        let first = &full_entries[0];

        // MA-5: average of values at times 1296-1300
        let ma_5_expected = (296..=300).map(|i| 100.0 + (i as f64 * 0.1)).sum::<f64>() / 5.0;
        assert!((first.ma_5.unwrap() - ma_5_expected).abs() < 1e-10);

        // MA-60: average of values at times 1241-1300
        let ma_60_expected = (241..=300).map(|i| 100.0 + (i as f64 * 0.1)).sum::<f64>() / 60.0;
        assert!((first.ma_60.unwrap() - ma_60_expected).abs() < 1e-10);

        // MA-300: average of values at times 1001-1300
        let ma_300_expected = (1..=300).map(|i| 100.0 + (i as f64 * 0.1)).sum::<f64>() / 300.0;
        assert!((first.ma_300.unwrap() - ma_300_expected).abs() < 1e-10);
    }

    #[test]
    fn test_entries_before_start_not_included() {
        // Create entries from 995 to 1005
        let mut entries = vec![];
        for i in 995..=1005 {
            entries.push(create_partial_entry(datetime_from_timestamp(i), i as f64));
        }

        let start_time = datetime_from_timestamp(1000);
        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_ok());

        let full_entries = result.unwrap();
        assert_eq!(full_entries.len(), 6); // Only entries from 1000 to 1005

        // Verify that the first entry is at time 1000
        assert_eq!(full_entries[0].time, datetime_from_timestamp(1000));
        assert_eq!(full_entries[0].value, 1000.0);

        // But the MA-5 should use values from before start_time
        assert!(full_entries[0].ma_5.is_some());
        let expected_ma5 = (996..=1000).map(|i| i as f64).sum::<f64>() / 5.0;
        assert_eq!(full_entries[0].ma_5.unwrap(), expected_ma5);
    }

    #[test]
    fn test_exact_window_boundary() {
        // Test when we have exactly the number of entries needed for each MA
        let mut entries = vec![];

        // Create exactly 5 entries
        for i in 0..5 {
            entries.push(create_partial_entry(
                datetime_from_timestamp(1000 + i),
                10.0,
            ));
        }

        let start_time = datetime_from_timestamp(1000);
        let result = IndicatorsEvaluator::evaluate(entries.clone(), start_time);
        assert!(result.is_ok());

        let full_entries = result.unwrap();

        // First 4 entries should not have MA-5
        for i in 0..4 {
            assert!(full_entries[i].ma_5.is_none());
        }

        // 5th entry should have MA-5
        assert!(full_entries[4].ma_5.is_some());
        assert_eq!(full_entries[4].ma_5.unwrap(), 10.0);

        // Test with exactly 60 entries
        entries.clear();
        for i in 0..60 {
            entries.push(create_partial_entry(
                datetime_from_timestamp(1000 + i),
                20.0,
            ));
        }

        let result = IndicatorsEvaluator::evaluate(entries, start_time);
        assert!(result.is_ok());

        let full_entries = result.unwrap();

        // Check that MA-60 appears at the 60th entry
        assert!(full_entries[59].ma_60.is_some());
        assert_eq!(full_entries[59].ma_60.unwrap(), 20.0);
    }
}

mod moving_average_evaluator {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let mut ma = MovingAverageEvaluator::new(NonZeroU64::new(3).unwrap());

        // Not enough values yet
        assert_eq!(ma.update(1.0), None);
        assert_eq!(ma.update(2.0), None);

        // Now we have 3 values
        assert_eq!(ma.update(3.0), Some(2.0)); // (1+2+3)/3 = 2

        // Window slides
        assert_eq!(ma.update(4.0), Some(3.0)); // (2+3+4)/3 = 3
        assert_eq!(ma.update(5.0), Some(4.0)); // (3+4+5)/3 = 4
    }

    #[test]
    fn test_window_size_one() {
        let mut ma = MovingAverageEvaluator::new(NonZeroU64::new(1).unwrap());

        assert_eq!(ma.update(5.0), Some(5.0));
        assert_eq!(ma.update(10.0), Some(10.0));
        assert_eq!(ma.update(15.0), Some(15.0));
    }

    #[test]
    fn test_floating_point_precision() {
        let mut ma = MovingAverageEvaluator::new(NonZeroU64::new(3).unwrap());

        assert_eq!(ma.update(0.1), None);
        assert_eq!(ma.update(0.2), None);

        let result = ma.update(0.3);
        assert!(result.is_some());
        assert!((result.unwrap() - 0.2).abs() < 1e-10);
    }
}
