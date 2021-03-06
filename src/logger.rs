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

use crate::api_client::SolvedPuzzleStats;
use anyhow::Result;
use chrono::{naive::NaiveDate, Datelike, Weekday};
use indicatif::ProgressBar;
use serde::Serialize;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Serialize)]
pub struct PuzzleStats {
    date: NaiveDate,
    weekday: Weekday,
    // It would be nice to embed SolvedPuzzleStats here, but serde's flatten attribute doesn't play
    // well with the csv crate
    solve_time_secs: u32,
    opened_unix: Option<u32>,
    solved_unix: Option<u32>,
}

impl PuzzleStats {
    pub fn new(date: NaiveDate, solve_stats: SolvedPuzzleStats) -> Self {
        let weekday = date.weekday();
        Self {
            date,
            weekday,
            solve_time_secs: solve_stats.solve_time,
            opened_unix: solve_stats.opened,
            solved_unix: solve_stats.solved,
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq)]
pub enum Payload {
    Solve(PuzzleStats),
    Unsolved,
    FetchError,
    Finished,
}

pub async fn task_fn(
    mut rx: mpsc::UnboundedReceiver<Payload>,
    progress: ProgressBar,
) -> Result<()> {
    // The csv crate's Writer will add a header row using struct fieldnames by default
    let mut writer = csv::Writer::from_path("xword.csv")?;
    while let Some(payload) = rx.recv().await {
        match payload {
            Payload::Solve(stats) => writer.serialize(stats).expect("Serialization error"),
            Payload::Finished => {
                progress.finish_with_message("All done 🎉");
                break;
            }
            _ => (),
        }
        progress.inc(1);
    }
    Ok(())
}
