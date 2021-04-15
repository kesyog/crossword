// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod api_client;
mod database;
mod logger;

use anyhow::Result;
use api_client::RateLimitedClient;
use chrono::naive::NaiveDate;
use core::num::NonZeroU32;
use database::{Database, PuzzleStats};
use futures::future;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, warn};
use std::convert::TryInto;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::sync::mpsc;

// Size of each block of dates to fetch metadata about. Currently hard-coded to match the expected
// server response with no validation that this is correct.
const DAY_STEP: u32 = 100;

#[derive(Debug, StructOpt)]
struct Opt {
    /// NYT subscription token extracted from web browser
    #[structopt(short = "t", long = "token", env = "NYT_S")]
    nyt_token: String,

    /// Earliest puzzle date to pull results from in YYYY-MM-DD format
    #[structopt(short, long, env = "NYT_XWORD_START")]
    start_date: NaiveDate,

    /// Rate-limit (per second) for outgoing requests
    #[structopt(
        short = "q",
        long = "quota",
        default_value = "5",
        env = "NYT_REQUESTS_PER_SEC"
    )]
    request_quota: NonZeroU32,

    /// Path to data from a previous program run ot use as a cache. If provided, results will only
    /// be fetched for puzzles that aren't already in the cache.
    #[structopt(short = "c", long = "cache")]
    input_file: Option<PathBuf>,

    /// Path to write CSV output. Can be the same as `input_file`
    output_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    let opt = Opt::from_args();

    let today = chrono::offset::Utc::today().naive_utc();
    let stats_db = match opt.input_file {
        Some(input) => Database::from_file(input, &opt.output_file).unwrap(),
        None => Database::new(&opt.output_file),
    };
    let search_space = stats_db.search_space(opt.start_date, today, DAY_STEP);

    let total_days: usize = search_space.iter().map(Vec::len).sum();
    let progress = ProgressBar::new(total_days.try_into().unwrap()).with_style(
        ProgressStyle::default_bar()
            .template("▕{bar:40}▏{eta} {percent}% {msg}")
            .progress_chars("⬛🔲⬜"),
    );
    progress.println(
        [
            "Fetching NYT crossword stats since",
            &opt.start_date.to_string(),
        ]
        .join(" "),
    );

    let (tx, rx) = mpsc::unbounded_channel();
    let logger_handle = tokio::spawn(logger::task_fn(rx, stats_db, progress));

    let client = RateLimitedClient::new(&opt.nyt_token, opt.request_quota);
    if let Err(e) = fetch_stats(client, search_space, tx.clone()).await {
        error!("fetch_stats returned error: {}", e);
    };
    tx.send(logger::Payload::Finished)?;
    logger_handle.await??;
    Ok(())
}

/// Concurrently fetch statistics for the crosswords from the given dates and send the results to
/// the provided channel
///
/// # Arguments
///
/// * `client` - A `RateLimitedClient` that can be used to send outgoing requests
/// * `dates` - Blocks of dates to search. Each block must be sorted and contain no more than
/// `DAY_STEP` elements
/// * `logger` - Channel where individual puzzle's statistics should be sent to
async fn fetch_stats(
    client: RateLimitedClient,
    dates: Vec<Vec<NaiveDate>>,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    let mut futures = Vec::new();
    for block_of_dates in dates {
        futures.push(tokio::spawn({
            let client = client.clone();
            let tx = logger.clone();
            async move { search_date_block(client, block_of_dates, tx).await }
        }));
    }

    future::join_all(futures).await;
    Ok(())
}

/// Concurrently search crosswords within the provided block of dates and send the results to the
/// provided channel
///
/// # Arguments
///
/// * `client` - A `RateLimitedClient` that can be used to send outgoing requests
/// * `block_of_dates` - Sorted list of puzzle dates to search. Must contain no more than
/// `DAY_STEP` elements
/// * `logger` - Channel where individual puzzle's statistics should be sent to
async fn search_date_block(
    client: RateLimitedClient,
    block_of_dates: Vec<NaiveDate>,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    assert!(block_of_dates.len() <= DAY_STEP.try_into().unwrap());
    let start = block_of_dates[0];
    let end = *block_of_dates.iter().last().unwrap();

    debug!("Fetching ids for date range {} to {}", start, end);
    let id_map = match client.get_puzzle_ids(start, end).await {
        Ok(map) => map,
        Err(e) => {
            // This may occur if the entire date block consists of unreleased puzzles, which would
            // happen if the puzzle from the last date in the search block (today in UTC) hasn't
            // been released yet.
            warn!(
                "Couldn't get puzzle id for date range {} to {}. Error: {:?}",
                start, end, e
            );
            return Ok(());
        }
    };

    // Concurrently find stats for all puzzles in block
    let mut futures = Vec::new();
    for date in block_of_dates {
        let id = if let Some(id) = id_map.get(&date) {
            *id
        } else {
            // This will occur if there are unreleased puzzles in this date block
            warn!("No id found for {}", date);
            continue;
        };
        futures.push(tokio::spawn({
            let client = client.clone();
            let logger = logger.clone();
            async move {
                match client.get_solve_stats(id).await {
                    Ok(Some(solve_stats)) => {
                        let stats = PuzzleStats::new(date, id, Some(solve_stats));
                        logger
                            .send(logger::Payload::Solve(stats))
                            .expect("Failed to send result to channel");
                    }
                    Ok(None) => {
                        let stats = PuzzleStats::new(date, id, None);
                        logger
                            .send(logger::Payload::Unsolved(stats))
                            .expect("Failed to send result to channel");
                    }
                    Err(e) => {
                        error!("Failed to get stats for date={} id={}: {}", date, id, e);
                        logger
                            .send(logger::Payload::FetchError)
                            .expect("Failed to send result to channel");
                    }
                }
            }
        }));
    }
    future::join_all(futures).await;
    Ok(())
}
