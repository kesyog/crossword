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

use crate::database::{Database, PuzzleStats};
use anyhow::Result;
use indicatif::ProgressBar;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, Hash, PartialEq)]
pub enum Payload {
    Solve(PuzzleStats),
    Unsolved,
    FetchError,
    Finished,
}

pub async fn task_fn(
    mut rx: mpsc::UnboundedReceiver<Payload>,
    mut stats_db: Database,
    progress: ProgressBar,
) -> Result<()> {
    while let Some(payload) = rx.recv().await {
        match payload {
            Payload::Solve(stats) => stats_db.add(stats),
            Payload::Finished => {
                progress.finish_with_message("All done 🎉");
                break;
            }
            Payload::Unsolved | Payload::FetchError => (),
        }
        progress.inc(1);
    }
    Ok(())
}
