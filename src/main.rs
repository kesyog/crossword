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

use anyhow::Result;
use chrono::{naive::NaiveDate, Duration};
use core::num::NonZeroU32;
use crossword::api_client::RateLimitedClient;
use crossword::database::Database;
use crossword::{logger, DAY_STEP};
use indicatif::{ProgressBar, ProgressStyle};
use log::warn;
use std::convert::TryInto;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::sync::mpsc;

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

    /// Path to write CSV output. If a CSV file from a previous program exists at that path, it
    /// will be updated with missing data and the number of requests made will potentially be
    /// reduced.
    db_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    let opt = Opt::from_args();

    let today = chrono::offset::Utc::now().date_naive();
    let stats_db = if opt.db_path.exists() {
        Database::from_file(opt.db_path)
            .expect("Given file exists but does not contain a valid database")
    } else {
        Database::new(opt.db_path)
    };

    let missing_ids = crossword::get_days_without_ids_chunked(
        &stats_db,
        opt.start_date,
        today,
        Duration::days(DAY_STEP),
    );
    let cached_unsolved = crossword::get_cached_unsolved_records(&stats_db, opt.start_date);

    let total_days = missing_ids.iter().map(Vec::len).sum::<usize>() + cached_unsolved.len();
    let progress = ProgressBar::new(total_days.try_into().unwrap()).with_style(
        ProgressStyle::default_bar()
            .template("▕{bar:40}▏{eta} {percent}% {msg}")
            .progress_chars("⬛🔲⬜"),
    );

    let msg = format!(
        "Fetching NYT crossword stats since {}",
        &opt.start_date.to_string()
    );
    progress.println(msg);

    let (tx, rx) = mpsc::unbounded_channel();
    let logger_handle = tokio::spawn(logger::task_fn(rx, stats_db, progress));

    let client = RateLimitedClient::new(&opt.nyt_token, opt.request_quota);

    let ids_task = tokio::spawn(crossword::search::fetch_ids_and_stats(
        client.clone(),
        missing_ids,
        tx.clone(),
    ));
    let unsolved_task = tokio::spawn(crossword::search::fetch_missing_times(
        client.clone(),
        cached_unsolved,
        tx.clone(),
    ));

    if let Err(e) = ids_task.await? {
        warn!("Error in fetch_ids_and_stats: {}", e);
    };
    if let Err(e) = unsolved_task.await? {
        warn!("Error in fetch_missing_times: {}", e);
    };
    tx.send(logger::Payload::Finished(client.n_requests()))?;
    logger_handle.await??;
    Ok(())
}
