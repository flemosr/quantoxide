use chrono::{DateTime, Duration, Utc};
use std::{collections::HashSet, thread, time};

use crate::{
    api::{models::PriceEntryLNM, rest},
    db::DB,
    env::{
        LNM_API_COOLDOWN_SEC, LNM_API_ERROR_COOLDOWN_SEC, LNM_API_ERROR_MAX_TRIALS,
        LNM_MIN_PRICE_HISTORY_WEEKS, LNM_PRICE_HISTORY_LIMIT,
    },
    Result,
};

fn wait(secs: u64) {
    thread::sleep(time::Duration::from_secs(secs));
}

enum LimitReached {
    No,
    Yes { overlap: bool },
}

async fn get_new_price_entries(
    limit: &DateTime<Utc>,
    before_observed_time: Option<DateTime<Utc>>,
) -> Result<(Vec<PriceEntryLNM>, LimitReached)> {
    let mut price_entries = {
        let mut trials = 0;

        loop {
            wait(*LNM_API_COOLDOWN_SEC);

            match rest::futures_price_history(
                None,
                before_observed_time,
                Some(*LNM_PRICE_HISTORY_LIMIT),
            )
            .await
            {
                Ok(price_entries) => break price_entries,
                Err(e) => {
                    println!("\nError fetching price history {:?}", e);

                    trials += 1;
                    if trials >= *LNM_API_ERROR_MAX_TRIALS {
                        return Err(e);
                    }

                    println!(
                        "Remaining trials: {}. Waiting {} secs...",
                        *LNM_API_ERROR_MAX_TRIALS - trials,
                        *LNM_API_ERROR_COOLDOWN_SEC
                    );

                    wait(*LNM_API_ERROR_COOLDOWN_SEC);

                    continue;
                }
            };
        }
    };

    if price_entries.len() < *LNM_PRICE_HISTORY_LIMIT {
        println!(
            "\nReceived only {} price entries with limit {}.",
            price_entries.len(),
            *LNM_PRICE_HISTORY_LIMIT
        );
    }

    // Remove entries with duplicated 'time'
    let mut seen = HashSet::new();
    price_entries.retain(|price_entry| seen.insert(*price_entry.time()));

    let is_sorted_time_desc = price_entries.is_sorted_by(|a, b| a.time() > b.time());
    if is_sorted_time_desc == false {
        return Err("Got price entries unsorted by time desc".into());
    }

    // If `before_observed_time` is set, ensure that the first (latest) entry matches it
    if let Some(observed_time) = before_observed_time {
        let first_entry = price_entries.remove(0);
        if *first_entry.time() != observed_time {
            return Err("Got price entries without overlap.".into());
        }
        println!(
            "First received entry matches `before_observed_time` time {}. Overlap OK.",
            observed_time
        );
    }

    let limit_reached = if let Some(entry_i) = price_entries
        .iter()
        .position(|price_entry| price_entry.time() <= limit)
    {
        // Remove the entries before the `limit`
        let before_limit = price_entries.split_off(entry_i);
        let overlap = before_limit.first().expect("not empty").time() == limit;
        LimitReached::Yes { overlap }
    } else {
        LimitReached::No
    };

    Ok((price_entries, limit_reached))
}

async fn download_price_history(
    limit: &DateTime<Utc>,
    mut next_observed_time: Option<DateTime<Utc>>,
) -> Result<bool> {
    let limit_next_observed_time = loop {
        match next_observed_time {
            Some(time) => println!("\nFetching price entries before {time}..."),
            None => println!("\nFetching latest price entries..."),
        }

        let (new_price_entries, limit_check) =
            get_new_price_entries(limit, next_observed_time).await?;

        if new_price_entries.is_empty() {
            println!("\nNo new entries were received.");
        } else {
            let entries_len = new_price_entries.len();
            let latest_new_entry_time = new_price_entries.first().expect("not empty").time();
            let earliest_new_entry_time = new_price_entries.last().expect("not empty").time();
            println!("\n{entries_len} new entries received, from {earliest_new_entry_time} to {latest_new_entry_time}");

            DB.add_price_entries(&new_price_entries, next_observed_time.as_ref())
                .await?;

            println!("\nEntries added to the db");
        }

        if let LimitReached::Yes { overlap } = limit_check {
            if overlap == false {
                // Received an entry with time before `limit`, but not `limit`
                break None;
            }

            // The `limit` price entry `next` value is now known, and the entry
            // must be updated in the db.

            if let Some(earliest_new_entry) = new_price_entries.last() {
                break Some(*earliest_new_entry.time());
            } else if let Some(time) = next_observed_time {
                // If there is a `next_observed_time`, the first entry received
                // from the server matched its time (overlap enforcement).
                // From this, we can infer that there are no entries to be
                // fetched between `limit` and `next_observed_time` (edge case).
                break Some(time);
            } else {
                // No entries available after `limit`
                break None;
            }
        }

        // `limit` not reached

        let earliest_new_entry = new_price_entries.last().expect("not empty");
        next_observed_time = Some(*earliest_new_entry.time());
    };

    if let Some(next) = limit_next_observed_time {
        println!("\nReached `limit` {limit}. Updating the corresponding entry's `next` field");

        DB.update_price_entry_next(limit, &next).await?;
        return Ok(true);
    }

    Ok(false)
}

pub async fn start() -> Result<()> {
    let limit = Utc::now() - Duration::weeks(*LNM_MIN_PRICE_HISTORY_WEEKS as i64);

    println!(
        "\nPrice history sync limit: {} weeks",
        *LNM_MIN_PRICE_HISTORY_WEEKS
    );
    println!("Limit timestamp: {limit}");

    if let Some(earliest_price_entry_gap) = DB.get_earliest_price_entry_gap().await? {
        if earliest_price_entry_gap.time < limit {
            // There is a price gaps before `limit`. Since we shouldn't fetch
            // entries before `limit`, said gaps can't be closed, and therefore
            // the db can't be synced.
            return Err(format!(
                "Price gap after {} was found. DB can't be synced.",
                earliest_price_entry_gap.time
            )
            .into());
        }
    }

    if let Some(latest_price_entry) = DB.get_latest_price_entry().await? {
        while let Some(earliest_price_entry_gap) = DB.get_earliest_price_entry_gap().await? {
            if earliest_price_entry_gap.time == latest_price_entry.time {
                // Earliest price entry gap is the latest price entry
                println!("\nNo history gaps before the latest entry were found.");
                break;
            }

            println!("\nGap after {} was found.", earliest_price_entry_gap.time);

            let first_price_entry_after_gap = DB
                .get_first_price_entry_after(earliest_price_entry_gap.time)
                .await?;
            let first_price_entry_after_gap_time = first_price_entry_after_gap
                .expect("Gap entry is not latest entry.")
                .time;

            println!(
                "\nEarliest price entry after gap has time {first_price_entry_after_gap_time}."
            );

            println!("\nDownloading entries from {first_price_entry_after_gap_time} backwards, until closing the gap...");

            let overlap_reached = download_price_history(
                &earliest_price_entry_gap.time,
                Some(first_price_entry_after_gap_time),
            )
            .await?;

            if overlap_reached == false {
                return Err(format!(
                    "entry gap time {} not received from server",
                    earliest_price_entry_gap.time
                )
                .into());
            }
        }
    }

    if let Some(earliest_price_entry) = DB.get_earliest_price_entry().await? {
        if earliest_price_entry.time > limit {
            println!(
                "\nDownloading price entries from the earliest ({}) in the DB backwards, until limit ({})...",
                earliest_price_entry.time,
                limit
            );
            download_price_history(&limit, Some(earliest_price_entry.time)).await?;
        }
    } else {
        println!("\nNo price entries in the DB. Downloading from latest to earliest...");
        download_price_history(&limit, None).await?;
    }

    // We can assume that `limit` was reached, and the min history condition is
    // satisfied.

    let mut latest_price_entry = DB.get_latest_price_entry().await?.expect("db not empty");

    loop {
        println!(
            "\nDownloading the latest price entries until reaching the latest ({}) in the DB...",
            latest_price_entry.time
        );

        let additional_entries_observed =
            download_price_history(&latest_price_entry.time, None).await?;

        if additional_entries_observed == false {
            println!("\nNo new entries after {}", latest_price_entry.time);
            continue;
        }

        latest_price_entry = DB.get_latest_price_entry().await?.expect("db not empty");
    }
}
