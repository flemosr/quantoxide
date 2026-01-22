//! Input utility functions for examples.

#![allow(unused)]

use std::{
    collections::HashMap,
    env,
    io::{self, Write},
};

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

/// Parses command line arguments into a HashMap.
/// Supports `--key value` pairs, where keys without values get an empty string.
pub fn parse_args() -> HashMap<String, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut map = HashMap::new();

    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            let key = args[i][2..].to_string();
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                map.insert(key, args[i + 1].clone());
                i += 2;
            } else {
                map.insert(key, String::new());
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    map
}
