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

use anyhow::{Context, Result};
use chrono::{naive::NaiveDate, Duration};
use clap::{Args, Parser};
use core::num::NonZeroU32;
use crossword::api_client::{RateLimitedClient, SubscriptionToken};
use crossword::database::Database;
use crossword::{logger, DAY_STEP};
use indicatif::{ProgressBar, ProgressStyle};
use log::warn;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Parser)]
struct Opt {
    #[command(flatten)]
    subscription_token: NytToken,

    /// Earliest puzzle date to pull results from in YYYY-MM-DD format
    #[arg(short, long, env = "NYT_XWORD_START")]
    start_date: NaiveDate,

    /// Rate-limit (per second) for outgoing requests
    #[arg(
        short = 'q',
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

/// NYT subscription token extracted from web browser
#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
struct NytToken {
    /// NYT subscription token from nyt-s HTTP header
    #[arg(long, env = "NYT_S_HEADER")]
    nyt_header: Option<String>,
    /// NYT subscription token from NYT-S cookie
    #[arg(long, short = 't', env = "NYT_S_COOKIE")]
    nyt_cookie: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    let opt = Opt::parse();

    let today = chrono::offset::Utc::now().date_naive();
    let stats_db = if opt.db_path.exists() {
        Database::from_file(&opt.db_path).with_context(|| {
            format!(
                "Given file exists but does not contain a valid database: {}",
                opt.db_path.display()
            )
        })?
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
    let progress = ProgressBar::new(total_days.try_into()?).with_style(
        ProgressStyle::default_bar()
            .template("▕{bar:40}▏{eta} {percent}% {msg}")?
            .progress_chars("⬛🔲⬜"),
    );

    let msg = format!(
        "Fetching NYT crossword stats since {}",
        &opt.start_date.to_string()
    );
    progress.println(msg);

    let (tx, rx) = mpsc::unbounded_channel();
    let logger_handle = tokio::spawn(logger::task_fn(rx, stats_db, progress));

    let token = if let Some(header) = opt.subscription_token.nyt_header {
        SubscriptionToken::Header(header)
    } else if let Some(cookie) = opt.subscription_token.nyt_cookie {
        SubscriptionToken::Cookie(cookie)
    } else {
        anyhow::bail!("No NYT subscription token provided");
    };
    let client = RateLimitedClient::new(token, opt.request_quota);

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
