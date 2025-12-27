//! Input utility functions for examples.

#![allow(unused)]

use std::io::{self, Write};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

use quantoxide::error::Result;

/// Prompts the user and collects trimmed input from stdin.
fn collect_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_string())
}

/// Parses a date string in YYYY-MM-DD format into a UTC datetime at midnight.
pub fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
    let naive_datetime = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?
        .and_hms_opt(0, 0, 0)
        .ok_or("Failed to create datetime")?;

    Ok(Utc.from_utc_datetime(&naive_datetime))
}

/// Parses a balance string into a u64 value.
fn parse_balance(balance_str: &str) -> Result<u64> {
    Ok(balance_str.parse::<u64>()?)
}

/// Prompts for a date in YYYY-MM-DD format, re-prompting on invalid input.
pub fn prompt_date(prompt: &str) -> Result<DateTime<Utc>> {
    loop {
        let input = collect_input(prompt)?;
        match parse_date(&input) {
            Ok(dt) => return Ok(dt),
            Err(e) => {
                println!("Error: {}. Please try again.", e);
                continue;
            }
        }
    }
}

/// Prompts for a numeric balance, returning the default if empty, re-prompting on invalid input.
pub fn prompt_balance(prompt: &str, default: u64) -> Result<u64> {
    loop {
        let input = collect_input(prompt)?;
        if input.is_empty() {
            return Ok(default);
        }
        match parse_balance(&input) {
            Ok(balance) => return Ok(balance),
            Err(e) => {
                println!("Error: {}. Please try again.", e);
                continue;
            }
        }
    }
}
