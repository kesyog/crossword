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
mod logger;

use anyhow::Result;
use api_client::RateLimitedClient;
use chrono::naive::NaiveDate;
use chrono::Duration;
use futures::future;
use indicatif::{ProgressBar, ProgressStyle};
use std::cmp;
use std::convert::{From, Into, TryInto};
use tokio::sync::mpsc;

// Size of each block of dates to fetch metadata about. Currently hard-coded to match the expected
// server response with no validation that this is correct.
const DAY_STEP: u32 = 100;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let today = chrono::offset::Utc::today().naive_utc();
    let start_date_str = dotenv::var("NYT_XWORD_START").expect("Provide search start date in YYYY-MM-DD format as an environment variable named NYT_XWORD_START.");
    let start_date: NaiveDate = start_date_str
        .parse()
        .expect("Start date parse error. Please provide date in YYYY-MM-DD format.");
    let total_days = (today - start_date + Duration::days(1)).num_days();
    let progress = ProgressBar::new(total_days.try_into().unwrap()).with_style(
        ProgressStyle::default_bar()
            .template("▕{bar:40}▏{eta} {percent}% {msg}")
            .progress_chars("⬛🔲⬜"),
    );
    progress.println(["Fetching NYT crossword stats since ", &start_date_str].join(""));

    let (tx, rx) = mpsc::unbounded_channel();
    let logger_handle = tokio::spawn(logger::task_fn(rx, progress));

    let client = RateLimitedClient::new();
    let mut futures = Vec::new();
    let mut date = start_date;
    while date <= today {
        let end_date = cmp::min(today, date + Duration::days(i64::from(DAY_STEP) - 1));
        futures.push(tokio::spawn({
            let client = client.clone();
            let tx = tx.clone();
            async move { client.search_dates(date, end_date, tx).await }
        }));
        date += Duration::days(DAY_STEP.into());
    }

    future::join_all(futures).await;
    tx.send(logger::Payload::Finished)?;
    logger_handle.await??;
    Ok(())
}
